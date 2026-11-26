//! Full-duplex voice audio for calls: echo-cancelled microphone frames in, speaker frames out,
//! both as mono `f32` PCM at a rate the app picks.
//!
//! [`Voice::open`] returns a control handle and two lock-free stream ends. [`VoiceMic`] reads
//! what the microphone heard with the speaker's own output removed. [`VoiceSpeaker`] queues
//! what to play. The device's audio thread touches only the two rings and a few atomics, so it
//! never allocates, locks or waits on the app.
//!
//! On macOS and iOS the stream runs through Apple's voice-processing I/O unit, which cancels
//! echo, suppresses noise and levels the microphone. On iOS the audio session is set to
//! play-and-record in voice-chat mode, using the loudspeaker unless headphones or a Bluetooth
//! headset are connected. When the default devices change (headphones plugged in or out) the
//! stream rebuilds itself on the new route. When a call or another app interrupts it, it pauses
//! and resumes afterwards. On Windows, Linux and the web, [`Voice::open`] returns
//! [`VoiceError::Unsupported`]. [`Voice::open_fake`] runs anywhere, on a [`FakeVoice`].
//!
//! Apps reach these types through `rune`, which re-exports them.

mod control;
mod fake;
mod io;
mod machine;
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod apple;
#[cfg(not(any(target_os = "macos", target_os = "ios")))]
mod unsupported;

#[cfg(any(target_os = "macos", target_os = "ios"))]
use apple as backend;
#[cfg(not(any(target_os = "macos", target_os = "ios")))]
use unsupported as backend;

pub use fake::FakeVoice;
pub use io::{VoiceMic, VoiceSpeaker};

use crate::recorder::MicPermission;
use control::{Board, Control, Signal, SignalSender, VoiceDevice};
use io::{Capture, Meters, Playback};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

const MIN_RATE: u32 = 8_000;
const MAX_RATE: u32 = 48_000;

/// How to open a [`Voice`]: the sample rate both streams use, and how much each end buffers.
///
/// ```
/// use rune_media::VoiceSpec;
/// use std::time::Duration;
///
/// let spec = VoiceSpec::new(24_000).mic_buffer(Duration::from_millis(300)).speaker_buffer(Duration::from_secs(5));
/// # let _ = spec;
/// ```
#[derive(Clone)]
pub struct VoiceSpec {
    sample_rate: u32,
    mic_buffer: Duration,
    speaker_buffer: Duration,
    waker: Option<rune_core::Waker>,
}

impl fmt::Debug for VoiceSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VoiceSpec")
            .field("sample_rate", &self.sample_rate)
            .field("mic_buffer", &self.mic_buffer)
            .field("speaker_buffer", &self.speaker_buffer)
            .finish()
    }
}

impl VoiceSpec {
    /// Mono `f32` at `sample_rate` Hz (8,000 to 48,000) both ways, with 200 ms of microphone
    /// and 2 s of speaker buffer.
    pub fn new(sample_rate: u32) -> Self {
        Self { sample_rate, mic_buffer: Duration::from_millis(200), speaker_buffer: Duration::from_secs(2), waker: None }
    }

    /// How much captured audio waits for [`VoiceMic::read`] before new frames are dropped
    /// (20 ms to 2 s).
    pub fn mic_buffer(mut self, duration: Duration) -> Self {
        self.mic_buffer = duration;
        self
    }

    /// How far ahead of playback [`VoiceSpeaker::write`] can queue (100 ms to 10 s).
    pub fn speaker_buffer(mut self, duration: Duration) -> Self {
        self.speaker_buffer = duration;
        self
    }

    /// Called from another thread when the state changes or an event arrives.
    pub fn waker(mut self, waker: rune_core::Waker) -> Self {
        self.waker = Some(waker);
        self
    }

    fn frames(&self, duration: Duration, min: Duration, max: Duration) -> usize {
        (duration.clamp(min, max).as_secs_f64() * f64::from(self.sample_rate)).ceil() as usize
    }

    fn validate(&self) -> Result<(), VoiceError> {
        if (MIN_RATE..=MAX_RATE).contains(&self.sample_rate) {
            Ok(())
        } else {
            Err(VoiceError::InvalidSpec(format!("sample rate {} Hz outside {MIN_RATE}..={MAX_RATE}", self.sample_rate)))
        }
    }
}

/// Why a [`Voice`] didn't open.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum VoiceError {
    InvalidSpec(String),
    /// This platform has no echo-cancelled voice I/O in Rune yet.
    Unsupported,
    OpenFailed(String),
}

impl fmt::Display for VoiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSpec(message) => write!(f, "invalid voice spec: {message}"),
            Self::Unsupported => write!(f, "voice audio isn't supported on this platform yet"),
            Self::OpenFailed(message) => write!(f, "couldn't open voice audio: {message}"),
        }
    }
}

impl std::error::Error for VoiceError {}

/// Where a [`Voice`] is.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum VoiceState {
    Idle,
    Starting,
    Running,
    /// The devices changed or the device failed; the stream is being rebuilt.
    Reconnecting,
    /// A call or another app has the audio; the stream resumes when it ends.
    Interrupted,
    /// The stream couldn't start or come back. [`Voice::start`] tries again.
    Failed(String),
}

/// Something that happened to a [`Voice`], from [`Voice::take_events`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum VoiceEvent {
    Started,
    Stopped,
    /// The stream moved to new devices and is running again.
    DeviceChanged,
    Interrupted,
    Resumed,
    Failed(String),
}

/// Counters from a [`Voice`]'s audio thread.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct VoiceStats {
    pub captured_frames: u64,
    pub played_frames: u64,
    pub mic_dropped_frames: u64,
    /// Device callbacks larger than the stream's buffers, which got silence.
    pub oversized_callbacks: u64,
    /// From the last [`Voice::start`] to the device's first callback.
    pub start_to_audio: Option<Duration>,
}

/// An echo-cancelled, full-duplex voice stream. Dropping it stops the stream and releases the
/// device; [`VoiceMic`] and [`VoiceSpeaker`] stay valid but carry no more audio.
///
/// ```no_run
/// use rune_media::{Voice, VoiceSpec};
///
/// let (voice, mut mic, mut speaker) = Voice::open(VoiceSpec::new(24_000))?;
/// voice.start();
/// let mut frames = [0.0f32; 480];
/// let n = mic.read(&mut frames);
/// speaker.write(&frames[..n]);
/// # Ok::<(), rune_media::VoiceError>(())
/// ```
pub struct Voice {
    signals: SignalSender,
    board: Arc<Board>,
    meters: Arc<Meters>,
    sample_rate: u32,
    thread: Option<JoinHandle<()>>,
}

impl fmt::Debug for Voice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Voice").field("sample_rate", &self.sample_rate).field("state", &self.state()).finish()
    }
}

impl Voice {
    /// Opens the system's echo-cancelled voice I/O on the default microphone and speaker. The
    /// device is prepared on a thread of its own, so this returns at once; call
    /// [`start`](Self::start) to begin. A bundled app needs `NSMicrophoneUsageDescription` in
    /// its Info.plist, and without one this fails instead of letting the system end the app.
    pub fn open(spec: VoiceSpec) -> Result<(Voice, VoiceMic, VoiceSpeaker), VoiceError> {
        spec.validate()?;
        backend::check()?;
        let rate = spec.sample_rate;
        Self::launch(spec, move |capture, playback, signals| backend::device(rate, capture, playback, signals))
    }

    /// Opens a stream on `fake` instead of a device, on any platform.
    pub fn open_fake(spec: VoiceSpec, fake: &FakeVoice) -> Result<(Voice, VoiceMic, VoiceSpeaker), VoiceError> {
        spec.validate()?;
        let (rate, fake) = (spec.sample_rate, fake.clone());
        Self::launch(spec, move |capture, playback, signals| fake.device(rate, capture, playback, signals))
    }

    fn launch(
        spec: VoiceSpec,
        device: impl FnOnce(Capture, Playback, SignalSender) -> Box<dyn VoiceDevice> + Send + 'static,
    ) -> Result<(Voice, VoiceMic, VoiceSpeaker), VoiceError> {
        let mic_frames = spec.frames(spec.mic_buffer, Duration::from_millis(20), Duration::from_secs(2));
        let speaker_frames = spec.frames(spec.speaker_buffer, Duration::from_millis(100), Duration::from_secs(10));
        let ends = io::ends(mic_frames, speaker_frames);
        let board = Arc::new(Board::new());
        if let Some(waker) = spec.waker {
            board.waker.set(waker);
        }
        let (signals, received) = mpsc::channel();
        let (capture, playback, meters) = (ends.capture, ends.playback, ends.meters.clone());
        let (thread_board, thread_signals) = (board.clone(), signals.clone());
        let thread = std::thread::Builder::new()
            .name("rune-voice".into())
            .spawn(move || {
                let device = device(capture, playback, thread_signals);
                Control::new(device, meters, thread_board).run(&received);
            })
            .map_err(|e| VoiceError::OpenFailed(e.to_string()))?;
        let voice = Voice { signals, board, meters: ends.meters, sample_rate: spec.sample_rate, thread: Some(thread) };
        Ok((voice, ends.mic, ends.speaker))
    }

    /// The rate both streams run at, in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Starts the microphone and speaker. Without microphone permission the system asks first,
    /// and the microphone hears silence until it is granted.
    pub fn start(&self) {
        self.meters.mark_start();
        self.send(Signal::Start);
    }

    /// Stops both streams. Queued speaker audio stays queued.
    pub fn stop(&self) {
        self.send(Signal::Stop);
    }

    /// Mutes the microphone: [`VoiceMic`] reads silence. The speaker keeps playing.
    pub fn set_muted(&self, muted: bool) {
        self.send(Signal::Mute(muted));
    }

    pub fn muted(&self) -> bool {
        self.meters.muted.load(Ordering::Relaxed)
    }

    pub fn state(&self) -> VoiceState {
        self.board.state()
    }

    /// Everything that happened since the last call, oldest first.
    pub fn take_events(&self) -> Vec<VoiceEvent> {
        self.board.take_events()
    }

    /// Installs a waker called from another thread when the state changes or an event arrives.
    pub fn set_waker(&self, waker: rune_core::Waker) {
        self.board.waker.set(waker);
        if let Some(generation) = self.board.pending() {
            self.board.waker.tell(generation);
        }
    }

    pub fn stats(&self) -> VoiceStats {
        self.meters.stats()
    }

    /// Microphone access now. Never asks.
    pub fn permission() -> MicPermission {
        backend::permission()
    }

    /// Asks for microphone access if it hasn't been decided, then calls `callback` with the
    /// answer, from any thread.
    pub fn request_permission(callback: impl FnOnce(MicPermission) + Send + 'static) {
        backend::request_permission(Box::new(callback));
    }

    fn send(&self, signal: Signal) {
        self.signals.send(signal).ok();
    }
}

impl Drop for Voice {
    fn drop(&mut self) {
        self.send(Signal::Close);
        if let Some(thread) = self.thread.take() {
            thread.join().ok();
        }
    }
}

#[cfg(test)]
mod io_tests;
#[cfg(test)]
mod machine_tests;
#[cfg(test)]
mod tests;
