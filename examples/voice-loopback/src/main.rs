use rune::{MicPermission, Voice, VoiceSpec, VoiceState};
use std::collections::VecDeque;
use std::process::exit;
use std::sync::mpsc;
use std::time::Duration;

const RATE: u32 = 48_000;
const TICK: Duration = Duration::from_millis(5);
const PERIOD: u64 = 2 * RATE as u64;
const BURST: u64 = RATE as u64 / 5;
const RAMP: u64 = RATE as u64 / 200;
const BURST_LEVEL: f32 = 0.3;
const MARGIN: u64 = RATE as u64 / 20;
const QUIET_FROM: u64 = BURST + RATE as u64 / 2;
const QUIET_TO: u64 = PERIOD - RATE as u64 / 5;
const TARGET_QUEUED: usize = RATE as usize / 25;

struct Args {
    loopback: bool,
    delay_ms: u64,
    seconds: Option<u64>,
}

fn args() -> Args {
    let mut args = Args { loopback: true, delay_ms: 300, seconds: None };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--burst-only" => args.loopback = false,
            "--delay-ms" => args.delay_ms = it.next().and_then(|v| v.parse().ok()).unwrap_or(args.delay_ms),
            "--seconds" => args.seconds = it.next().and_then(|v| v.parse().ok()),
            _ => {
                eprintln!("usage: voice-loopback [--burst-only] [--delay-ms N] [--seconds N]");
                exit(2);
            }
        }
    }
    args
}

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

fn permission() {
    match Voice::permission() {
        MicPermission::Granted => {}
        MicPermission::Undetermined => {
            let (tx, rx) = mpsc::channel();
            Voice::request_permission(move |answer| {
                tx.send(answer).ok();
            });
            if rx.recv() != Ok(MicPermission::Granted) {
                eprintln!("microphone access was not granted");
                exit(1);
            }
        }
        other => {
            let settings = if cfg!(target_os = "windows") {
                "Settings > Privacy & security > Microphone"
            } else {
                "System Settings > Privacy & Security > Microphone"
            };
            eprintln!("microphone access is {other:?}; allow it in {settings}");
            exit(1);
        }
    }
}

fn main() {
    let args = args();
    let opened = rune::time::now();
    let (voice, mut mic, mut speaker) = match Voice::open(VoiceSpec::new(RATE)) {
        Ok(stream) => stream,
        Err(e) => {
            eprintln!("{e}");
            exit(1);
        }
    };
    permission();
    voice.start();
    println!(
        "voice-loopback: {} Hz mono, a 200 ms noise burst every 2 s{}",
        RATE,
        if args.loopback { format!(", mic looped to the speaker {} ms late", args.delay_ms) } else { String::new() }
    );

    let delay = (args.delay_ms * u64::from(RATE) / 1_000) as usize;
    let mut line: VecDeque<f32> = VecDeque::with_capacity(delay + RATE as usize);
    let mut heard = vec![0.0f32; RATE as usize];
    let mut out = vec![0.0f32; RATE as usize];
    let mut written: u64 = 0;
    let mut echo = Echo::default();
    let mut state = VoiceState::Idle;
    let mut running_at: Option<Duration> = None;
    let mut next_report = rune::time::now() + Duration::from_secs(1);

    loop {
        let now = rune::time::now();
        let current = voice.state();
        if current != state {
            println!("[{:>7.3}s] state {current:?}", opened.elapsed().as_secs_f64());
            if current == VoiceState::Running && running_at.is_none() {
                running_at = Some(opened.elapsed());
            }
            state = current;
        }
        for event in voice.take_events() {
            println!("[{:>7.3}s] event {event:?}", opened.elapsed().as_secs_f64());
        }

        let n = mic.read(&mut heard);
        let played = speaker.played_frames();
        for (i, &sample) in heard[..n].iter().enumerate() {
            echo.hear((played + i as u64).saturating_sub(n as u64), sample);
        }
        if args.loopback {
            line.extend(&heard[..n]);
        }

        let need = TARGET_QUEUED.saturating_sub(speaker.queued()).min(out.len());
        for slot in &mut out[..need] {
            let looped = if line.len() > delay { line.pop_front().unwrap_or(0.0) } else { 0.0 };
            *slot = burst(written) + looped;
            written += 1;
        }
        speaker.write(&out[..need]);

        if now >= next_report {
            next_report += Duration::from_secs(1);
            let stats = voice.stats();
            let residual = echo
                .last
                .map(|(burst, quiet)| format!("burst echo {burst:.1} dB re burst, quiet floor {quiet:.1} dBFS"))
                .unwrap_or_else(|| "measuring echo".into());
            println!(
                "[{:>7.3}s] {state:?} | open->running {} | start->audio {} | captured {} played {} dropped {} oversized {} | {residual}",
                opened.elapsed().as_secs_f64(),
                running_at.map(|d| format!("{:.1} ms", d.as_secs_f64() * 1e3)).unwrap_or_else(|| "-".into()),
                stats.start_to_audio.map(|d| format!("{:.1} ms", d.as_secs_f64() * 1e3)).unwrap_or_else(|| "-".into()),
                stats.captured_frames,
                stats.played_frames,
                stats.mic_dropped_frames,
                stats.oversized_callbacks,
            );
        }
        if args.seconds.is_some_and(|s| opened.elapsed() >= Duration::from_secs(s)) {
            break;
        }
        std::thread::sleep(TICK);
    }
}
