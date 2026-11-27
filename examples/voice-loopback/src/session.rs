use rune::time::Instant;
use rune::{Voice, VoiceError, VoiceMic, VoiceSpeaker, VoiceSpec, VoiceState};
use std::collections::VecDeque;
use std::time::Duration;

pub const RATE: u32 = 48_000;
const PERIOD: u64 = 2 * RATE as u64;
const BURST: u64 = RATE as u64 / 5;
const RAMP: u64 = RATE as u64 / 200;
const BURST_LEVEL: f32 = 0.3;
const MARGIN: u64 = RATE as u64 / 20;
const QUIET_FROM: u64 = BURST + RATE as u64 / 2;
const QUIET_TO: u64 = PERIOD - RATE as u64 / 5;
const TARGET_QUEUED: usize = RATE as usize / 25;
const LINE_SLACK: usize = RATE as usize * 3 / 100;

fn noise(s: u64) -> f32 {
    let mut x = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x >> 40) as f32 / (1u64 << 23) as f32 - 1.0
}

fn burst(s: u64) -> f32 {
    let at = s % PERIOD;
    if at >= BURST {
        return 0.0;
    }
    let envelope = (at.min(BURST - at) as f32 / RAMP as f32).min(1.0);
    BURST_LEVEL * envelope * noise(s)
}

fn db(rms: f64) -> f64 {
    20.0 * rms.max(1e-9).log10()
}

#[derive(Default)]
struct Echo {
    burst: (f64, u64),
    quiet: (f64, u64),
    period: u64,
    last: Option<(f64, f64)>,
}

impl Echo {
    fn hear(&mut self, played: u64, sample: f32) {
        let period = played / PERIOD;
        if period != self.period {
            if self.burst.1 > 0 && self.quiet.1 > 0 {
                let burst = (self.burst.0 / self.burst.1 as f64).sqrt();
                let quiet = (self.quiet.0 / self.quiet.1 as f64).sqrt();
                let reference = f64::from(BURST_LEVEL) / 3f64.sqrt();
                self.last = Some((db(burst / reference), db(quiet)));
            }
            *self = Self { period, last: self.last, ..Self::default() };
        }
        let at = played % PERIOD;
        let energy = f64::from(sample) * f64::from(sample);
        if (MARGIN..BURST + MARGIN).contains(&at) {
            self.burst.0 += energy;
            self.burst.1 += 1;
        } else if (QUIET_FROM..QUIET_TO).contains(&at) {
            self.quiet.0 += energy;
            self.quiet.1 += 1;
        }
    }
}

pub struct Session {
    voice: Voice,
    mic: VoiceMic,
    speaker: VoiceSpeaker,
    loopback: bool,
    delay: usize,
    line: VecDeque<f32>,
    heard: Vec<f32>,
    out: Vec<f32>,
    written: u64,
    echo: Echo,
    level: (f64, u64),
    state: VoiceState,
    opened: Instant,
    running_at: Option<Duration>,
}

impl Session {
    pub fn open(loopback: bool, delay_ms: u64) -> Result<Self, VoiceError> {
        let opened = rune::time::now();
        let (voice, mic, speaker) = Voice::open(VoiceSpec::new(RATE))?;
        let delay = (delay_ms * u64::from(RATE) / 1_000) as usize;
        Ok(Self {
            voice,
            mic,
            speaker,
            loopback,
            delay,
            line: VecDeque::with_capacity(delay + RATE as usize),
            heard: vec![0.0; RATE as usize],
            out: vec![0.0; RATE as usize],
            written: 0,
            echo: Echo::default(),
            level: (0.0, 0),
            state: VoiceState::Idle,
            opened,
            running_at: None,
        })
    }

    pub fn start(&self) {
        self.voice.start();
    }

    pub fn elapsed(&self) -> Duration {
        rune::time::now().saturating_duration_since(self.opened)
    }

    pub fn banner(&self) -> String {
        let looped = if self.loopback { format!(", mic looped to the speaker {} ms late", self.delay * 1_000 / RATE as usize) } else { String::new() };
        format!("voice-loopback: {RATE} Hz mono, a 200 ms noise burst every 2 s{looped}")
    }

    pub fn tick(&mut self, log: &mut Vec<String>) {
        let at = self.elapsed().as_secs_f64();
        let current = self.voice.state();
        if current != self.state {
            log.push(format!("[{at:>7.3}s] state {current:?}"));
            if current == VoiceState::Running && self.running_at.is_none() {
                self.running_at = Some(self.elapsed());
            }
            self.state = current;
        }
        for event in self.voice.take_events() {
            log.push(format!("[{at:>7.3}s] event {event:?}"));
        }

        let n = self.mic.read(&mut self.heard);
        let played = self.speaker.played_frames();
        for (i, &sample) in self.heard[..n].iter().enumerate() {
            self.echo.hear((played + i as u64).saturating_sub(n as u64), sample);
            self.level.0 += f64::from(sample) * f64::from(sample);
        }
        self.level.1 += n as u64;
        if self.loopback {
            self.line.extend(&self.heard[..n]);
        }

        let need = TARGET_QUEUED.saturating_sub(self.speaker.queued()).min(self.out.len());
        for slot in &mut self.out[..need] {
            let looped = if self.line.len() > self.delay { self.line.pop_front().unwrap_or(0.0) } else { 0.0 };
            *slot = burst(self.written) + looped;
            self.written += 1;
        }
        self.speaker.write(&self.out[..need]);
        let excess = self.line.len().saturating_sub(self.delay + LINE_SLACK);
        self.line.drain(..excess);
    }

    pub fn report(&mut self) -> String {
        let stats = self.voice.stats();
        let residual = self
            .echo
            .last
            .map(|(burst, quiet)| format!("burst echo {burst:.1} dB re burst, quiet floor {quiet:.1} dBFS"))
            .unwrap_or_else(|| "measuring echo".into());
        let level = if self.level.1 > 0 { db((self.level.0 / self.level.1 as f64).sqrt()) } else { db(0.0) };
        self.level = (0.0, 0);
        let ms = |d: Option<Duration>| d.map(|d| format!("{:.1} ms", d.as_secs_f64() * 1e3)).unwrap_or_else(|| "-".into());
        let canceller = stats
            .echo_canceller
            .map(|c| {
                let (low, high) = c.reference_lead_frames;
                format!("aec erle {:.1} dB erl {:.1} dB aligned {} ms lead {low}..{high}", c.erle_db, c.erl_db, c.alignment.as_millis())
            })
            .unwrap_or_else(|| "aec -".into());
        let skew = i128::from(stats.captured_frames) - i128::from(stats.played_frames);
        format!(
            "[{:>7.3}s] {:?} | open->running {} | start->audio {} | captured {} played {} (captured-played {skew}) dropped {} oversized {} late {} speaker gaps {} mic gaps {} | mic {level:.1} dBFS | {canceller} | {residual}",
            self.elapsed().as_secs_f64(),
            self.state,
            ms(self.running_at),
            ms(stats.start_to_audio),
            stats.captured_frames,
            stats.played_frames,
            stats.mic_dropped_frames,
            stats.oversized_callbacks,
            stats.late_frames,
            stats.speaker_gap_frames,
            stats.mic_gap_frames,
        )
    }
}
