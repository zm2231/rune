use super::control::{Signal, SignalSender, VoiceDevice};
use super::io::{Capture, Playback};
use super::machine::Fault;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

pub(crate) const MAX_TICK: usize = 4096;

/// A stand-in audio device for tests and headless runs: no hardware, and audio moves only when
/// [`tick`](Self::tick) runs one device callback. The fake microphone hears what
/// [`say`](Self::say) queued plus the speaker's output, scaled by the echo gain and late by the
/// echo delay, the way an open speaker leaks into a microphone without echo cancellation.
///
/// ```
/// use rune_media::{FakeVoice, Voice, VoiceSpec};
///
/// let fake = FakeVoice::new().echo(0.5, std::time::Duration::ZERO);
/// let (voice, mut mic, mut speaker) = Voice::open_fake(VoiceSpec::new(16_000), &fake)?;
/// voice.start();
/// # while voice.state() != rune_media::VoiceState::Running { std::thread::yield_now(); }
/// speaker.write(&[0.8; 160]);
/// fake.tick(160);
/// let mut heard = [0.0; 160];
/// assert_eq!(mic.read(&mut heard), 160);
/// assert_eq!(heard[0], 0.4);
/// # Ok::<(), rune_media::VoiceError>(())
/// ```
#[derive(Clone)]
pub struct FakeVoice {
    inner: Arc<Mutex<Inner>>,
}

pub(crate) struct Inner {
    gain: f32,
    delay: Duration,
    line: Box<[f32]>,
    at: usize,
    near: VecDeque<f32>,
    out: Box<[f32]>,
    input: Box<[f32]>,
    last: usize,
    running: bool,
    pub(crate) failing_runs: u32,
    pub(crate) runs: u32,
    pub(crate) discards: u32,
    ends: Option<(Capture, Playback)>,
    signals: Option<SignalSender>,
}

impl Default for FakeVoice {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for FakeVoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let g = self.lock();
        f.debug_struct("FakeVoice").field("gain", &g.gain).field("delay", &g.delay).field("running", &g.running).finish()
    }
}

impl FakeVoice {
    /// A fake with no echo.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                gain: 0.0,
                delay: Duration::ZERO,
                line: Box::new([]),
                at: 0,
                near: VecDeque::new(),
                out: vec![0.0; MAX_TICK].into_boxed_slice(),
                input: vec![0.0; MAX_TICK].into_boxed_slice(),
                last: 0,
                running: false,
                failing_runs: 0,
                runs: 0,
                discards: 0,
                ends: None,
                signals: None,
            })),
        }
    }

    /// The share of the speaker's output the microphone hears, and how late.
    pub fn echo(self, gain: f32, delay: Duration) -> Self {
        {
            let mut g = self.lock();
            g.gain = if gain.is_finite() { gain } else { 0.0 };
            g.delay = delay;
        }
        self
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Queues near-end speech for the microphone to hear on the next ticks.
    pub fn say(&self, samples: &[f32]) {
        self.lock().near.extend(samples.iter().copied());
    }

    /// Runs one device callback of `frames` when the voice is running, and returns whether it ran:
    /// the speaker plays `frames` queued frames, then the microphone captures `frames`. A
    /// callback over 4096 frames moves no audio and counts in
    /// [`VoiceStats::oversized_callbacks`](super::VoiceStats::oversized_callbacks).
    pub fn tick(&self, frames: usize) -> bool {
        let mut g = self.lock();
        let g = &mut *g;
        let Some((capture, playback)) = g.ends.as_mut().filter(|_| g.running) else { return false };
        if frames > MAX_TICK {
            playback.meters.count_oversized();
            return true;
        }
        let out = &mut g.out[..frames];
        playback.render(out);
        for (slot, &played) in g.input[..frames].iter_mut().zip(out.iter()) {
            let echo = if g.line.is_empty() {
                played
            } else {
                let late = g.line[g.at];
                g.line[g.at] = played;
                g.at = (g.at + 1) % g.line.len();
                late
            };
            *slot = g.near.pop_front().unwrap_or(0.0) + g.gain * echo;
        }
        capture.capture(&g.input[..frames]);
        g.last = frames;
        true
    }

    /// What the speaker played on the last [`tick`](Self::tick).
    pub fn last_played(&self) -> Vec<f32> {
        let g = self.lock();
        g.out[..g.last].to_vec()
    }

    /// The system's default input or output device changes, as when headphones are plugged in.
    pub fn change_devices(&self) {
        self.signal(Signal::DevicesChanged);
    }

    /// Another app (a phone call, an alarm) takes the audio session.
    pub fn interrupt(&self) {
        self.signal(Signal::InterruptionBegan);
    }

    /// The interruption ends; `should_resume` is the system's hint that playing again is fine.
    pub fn end_interruption(&self, should_resume: bool) {
        self.signal(Signal::InterruptionEnded { should_resume });
    }

    /// The system's media services restart, which ends every audio session.
    pub fn reset_media(&self) {
        self.signal(Signal::MediaReset);
    }

    /// The app goes to the background.
    pub fn background(&self) {
        self.signal(Signal::Backgrounded);
    }

    /// The app comes back to the foreground.
    pub fn foreground(&self) {
        self.signal(Signal::Foregrounded);
    }

    fn signal(&self, signal: Signal) {
        if let Some(signals) = &self.lock().signals {
            signals.send(signal).ok();
        }
    }

    pub(crate) fn device(&self, sample_rate: u32, capture: Capture, playback: Playback, signals: SignalSender) -> Box<dyn VoiceDevice> {
        let mut g = self.lock();
        let delay_frames = (g.delay.as_secs_f64() * f64::from(sample_rate)).round() as usize;
        g.line = vec![0.0; delay_frames].into_boxed_slice();
        g.at = 0;
        g.running = false;
        g.ends = Some((capture, playback));
        g.signals = Some(signals);
        Box::new(FakeDevice { inner: self.inner.clone() })
    }
}

struct FakeDevice {
    inner: Arc<Mutex<Inner>>,
}

impl FakeDevice {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl VoiceDevice for FakeDevice {
    fn prepare(&mut self) -> Result<(), Fault> {
        Ok(())
    }

    fn run(&mut self) -> Result<(), Fault> {
        let mut g = self.lock();
        g.runs += 1;
        if g.failing_runs > 0 {
            g.failing_runs -= 1;
            return Err(Fault::passing("the fake device is unavailable"));
        }
        g.running = true;
        Ok(())
    }

    fn halt(&mut self) {
        self.lock().running = false;
    }

    fn discard(&mut self) {
        let mut g = self.lock();
        g.running = false;
        g.discards += 1;
    }
}
