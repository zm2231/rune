use super::echo_path::EchoPath;
use super::streams::{self, MicLink, Opened, Opener, SpeakerLink};
use super::machine::Fault;
use super::{Voice, VoiceEvent, VoiceMic, VoiceSpeaker, VoiceSpec, VoiceState};
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

pub(super) const RATE: u32 = 16_000;

const ECHO_DELAY: usize = 4 * RATE as usize / 100;

#[derive(Default)]
struct Air {
    heard_energy: Vec<f64>,
    seed: u32,
    fail_after: Option<usize>,
}

struct Line {
    sound: Mutex<Sound>,
    moved: Condvar,
}

struct Sound {
    echo: VecDeque<f32>,
    written: usize,
    read: usize,
    closed: bool,
}

impl Line {
    fn new() -> Arc<Self> {
        let echo = std::iter::repeat_n(0.0, ECHO_DELAY).collect();
        Arc::new(Self { sound: Mutex::new(Sound { echo, written: 0, read: 0, closed: false }), moved: Condvar::new() })
    }

    fn wait_until(&self, ready: impl Fn(&Sound) -> bool) -> std::sync::MutexGuard<'_, Sound> {
        let mut sound = self.sound.lock().unwrap();
        while !ready(&sound) && !sound.closed {
            sound = self.moved.wait(sound).unwrap();
        }
        sound
    }
}

struct Room {
    air: Arc<Mutex<Air>>,
    line: Arc<Line>,
}

impl Drop for Room {
    fn drop(&mut self) {
        self.line.sound.lock().unwrap().closed = true;
        self.line.moved.notify_all();
    }
}

impl MicLink for Room {
    fn read(&mut self, out: &mut [f32]) -> Result<(), Fault> {
        std::thread::sleep(Duration::from_micros(500));
        let mut air = self.air.lock().unwrap();
        if let Some(left) = air.fail_after.as_mut() {
            if *left == 0 {
                air.fail_after = None;
                return Err(Fault::passing("the sound server went away"));
            }
            *left -= 1;
        }
        let mut sound = self.line.wait_until(|sound| sound.echo.len() >= out.len());
        let mut energy = 0.0;
        for slot in out.iter_mut() {
            air.seed = air.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let hiss = ((air.seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.001;
            *slot = 0.5 * sound.echo.pop_front().unwrap_or(0.0) + hiss;
            energy += f64::from(*slot * *slot);
        }
        sound.read += out.len();
        drop(sound);
        self.line.moved.notify_all();
        air.heard_energy.push(energy);
        Ok(())
    }
}

impl SpeakerLink for Room {
    fn write(&mut self, frames: &[f32]) -> Result<(), Fault> {
        let mut sound = self.line.wait_until(|sound| sound.written <= sound.read);
        sound.echo.extend(frames);
        sound.written += frames.len();
        drop(sound);
        self.line.moved.notify_all();
        Ok(())
    }
}

fn room(air: &Arc<Mutex<Air>>) -> Opened {
    let line = Line::new();
    let (mic, speaker) = (Room { air: air.clone(), line: line.clone() }, Room { air: air.clone(), line });
    Opened { mic: Box::new(mic), speaker: Box::new(speaker), echo: EchoPath::InProcess }
}

fn opener(air: &Arc<Mutex<Air>>, mut failures: u32) -> Opener {
    let air = air.clone();
    Box::new(move |_, _| {
        if failures > 0 {
            failures -= 1;
            return Err(Fault::passing("connection refused"));
        }
        Ok(room(&air))
    })
}

fn open(open: Opener) -> (Voice, VoiceMic, VoiceSpeaker) {
    let spec = VoiceSpec::new(RATE).mic_buffer(Duration::from_secs(2)).speaker_buffer(Duration::from_secs(10));
    Voice::launch(spec, move |capture, playback, signals| streams::device(RATE, capture, playback, signals, open)).unwrap()
}

pub(super) fn until(voice: &Voice, want: VoiceState) {
    let deadline = rune_core::time::now() + Duration::from_secs(10);
    while voice.state() != want {
        assert!(rune_core::time::now() < deadline, "stuck in {:?} waiting for {want:?}", voice.state());
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub(super) fn speech(at: usize, seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(22_695_477).wrapping_add(1);
    let t = at as f32 / RATE as f32;
    0.4 * (t * 3.0).sin().abs() * (((*seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) + 0.5 * (std::f32::consts::TAU * 330.0 * t).sin())
}

#[test]
fn the_microphone_stream_has_the_speakers_echo_cancelled() {
    let air = Arc::new(Mutex::new(Air::default()));
    let (voice, mut mic, mut speaker) = open(opener(&air, 0));
    let seconds = 8;
    let mut seed = 3;
    let talk: Vec<f32> = (0..RATE as usize * seconds).map(|i| speech(i, &mut seed)).collect();
    assert_eq!(speaker.write(&talk), talk.len());
    voice.start();
    until(&voice, VoiceState::Running);
    let mut cleaned = Vec::new();
    let mut buf = vec![0.0; 4_096];
    while cleaned.len() < talk.len() {
        let n = mic.read(&mut buf);
        cleaned.extend(buf[..n].iter().map(|&s| f64::from(s * s)));
        std::thread::sleep(Duration::from_millis(1));
    }
    drop(voice);
    let half = cleaned.len() / 2;
    let kept: f64 = cleaned[half..].iter().sum();
    let heard: f64 = {
        let air = air.lock().unwrap();
        let blocks = air.heard_energy.len().min(cleaned.len() / (RATE as usize / 100));
        air.heard_energy[blocks / 2..blocks].iter().sum()
    };
    let reduction = 10.0 * (heard / kept.max(1e-20)).log10();
    assert!(reduction >= 25.0, "echo reduced by only {reduction:.1} dB");
    assert_eq!(mic.dropped_frames(), 0);
}

#[test]
fn a_sound_server_that_refuses_at_first_is_retried() {
    let air = Arc::new(Mutex::new(Air::default()));
    let (voice, _mic, _speaker) = open(opener(&air, 2));
    voice.start();
    until(&voice, VoiceState::Running);
    assert_eq!(voice.take_events(), vec![VoiceEvent::Started]);
}

#[test]
fn a_stream_that_dies_mid_call_comes_back() {
    let air = Arc::new(Mutex::new(Air { fail_after: Some(20), ..Air::default() }));
    let (voice, _mic, _speaker) = open(opener(&air, 0));
    voice.start();
    let deadline = rune_core::time::now() + Duration::from_secs(10);
    let mut events = Vec::new();
    while !events.contains(&VoiceEvent::DeviceChanged) {
        assert!(rune_core::time::now() < deadline, "no recovery: {events:?}, {:?}", voice.state());
        events.extend(voice.take_events());
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(voice.state(), VoiceState::Running);
}

#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
}

impl Gate {
    fn hold(&self) {
        let mut open = self.open.lock().unwrap();
        while !*open {
            open = self.changed.wait(open).unwrap();
        }
    }

    fn release(&self) {
        *self.open.lock().unwrap() = true;
        self.changed.notify_all();
    }
}

struct Stuck {
    gate: Arc<Gate>,
    fail: bool,
}

impl MicLink for Stuck {
    fn read(&mut self, _: &mut [f32]) -> Result<(), Fault> {
        if self.fail {
            return Err(Fault::passing("the microphone stream broke"));
        }
        self.gate.hold();
        Err(Fault::passing("released"))
    }
}

impl SpeakerLink for Stuck {
    fn write(&mut self, _: &[f32]) -> Result<(), Fault> {
        self.gate.hold();
        Err(Fault::passing("released"))
    }
}

fn stuck_then(air: &Arc<Mutex<Air>>, gate: &Arc<Gate>, mic_fails: bool) -> Opener {
    let (air, gate) = (air.clone(), gate.clone());
    let mut first = true;
    Box::new(move |_, _| {
        if std::mem::take(&mut first) {
            let mic = Stuck { gate: gate.clone(), fail: mic_fails };
            return Ok(Opened { mic: Box::new(mic), speaker: Box::new(Stuck { gate: gate.clone(), fail: false }), echo: EchoPath::InProcess });
        }
        Ok(room(&air))
    })
}

fn quickly(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = rune_core::time::now() + Duration::from_secs(3);
    while !done() {
        assert!(rune_core::time::now() < deadline, "{what} took too long");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn stopping_while_the_sound_server_blocks_returns_and_keeps_the_stream_usable() {
    let (air, gate) = (Arc::new(Mutex::new(Air::default())), Arc::new(Gate::default()));
    let (voice, _mic, _speaker) = open(stuck_then(&air, &gate, false));
    voice.start();
    until(&voice, VoiceState::Running);
    voice.stop();
    quickly("stopping", || voice.state() == VoiceState::Idle && !voice.take_events().is_empty());
    voice.start();
    until(&voice, VoiceState::Running);
    let captured = voice.stats().captured_frames;
    quickly("audio after restarting", || voice.stats().captured_frames > captured);
    let started = rune_core::time::now();
    drop(voice);
    assert!(rune_core::time::now() - started < Duration::from_secs(2));
    gate.release();
}

#[test]
fn a_fault_while_the_other_direction_blocks_still_rebuilds_the_stream() {
    let (air, gate) = (Arc::new(Mutex::new(Air::default())), Arc::new(Gate::default()));
    let (voice, _mic, _speaker) = open(stuck_then(&air, &gate, true));
    voice.start();
    let mut events = Vec::new();
    quickly("rebuilding", || {
        events.extend(voice.take_events());
        events.contains(&VoiceEvent::DeviceChanged)
    });
    assert_eq!(voice.state(), VoiceState::Running);
    gate.release();
}

#[test]
fn dropping_a_voice_whose_sound_server_blocks_returns() {
    let (air, gate) = (Arc::new(Mutex::new(Air::default())), Arc::new(Gate::default()));
    let (voice, _mic, _speaker) = open(stuck_then(&air, &gate, false));
    voice.start();
    until(&voice, VoiceState::Running);
    let started = rune_core::time::now();
    drop(voice);
    assert!(rune_core::time::now() - started < Duration::from_secs(2));
    gate.release();
}

#[test]
fn a_thread_that_fails_to_start_is_retried_with_the_queued_audio_kept() {
    for failing in 0..2 {
        let air = Arc::new(Mutex::new(Air::default()));
        let mut calls = 0;
        let spawner: streams::Spawner = Box::new(move |name, work| {
            calls += 1;
            if calls == failing + 1 {
                return Err(std::io::Error::other("out of threads"));
            }
            std::thread::Builder::new().name(name.into()).spawn(work).map(drop)
        });
        let spec = VoiceSpec::new(RATE).speaker_buffer(Duration::from_secs(1));
        let open = opener(&air, 0);
        let (voice, _mic, mut speaker) =
            Voice::launch(spec, move |capture, playback, signals| streams::with_spawner(RATE, capture, playback, signals, open, spawner)).unwrap();
        let queued = vec![0.1; 1_600];
        speaker.write(&queued);
        voice.start();
        until(&voice, VoiceState::Running);
        assert_eq!(voice.take_events(), vec![VoiceEvent::Started], "failing spawn #{}", failing + 1);
        quickly("playing the queued audio", || speaker.played_frames() >= queued.len() as u64);
    }
}

struct Positioned {
    link: Room,
    delivered: u64,
    ahead: i64,
    skip_at: Option<u64>,
}

impl MicLink for Positioned {
    fn read(&mut self, out: &mut [f32]) -> Result<(), Fault> {
        self.link.read(out)?;
        self.delivered += out.len() as u64;
        if Some(self.delivered) == self.skip_at {
            self.ahead += 480;
        }
        Ok(())
    }

    fn device_ahead(&self) -> Option<i64> {
        Some(self.ahead)
    }
}

impl SpeakerLink for Positioned {
    fn write(&mut self, frames: &[f32]) -> Result<(), Fault> {
        self.link.write(frames)
    }

    fn device_ahead(&self) -> Option<i64> {
        Some(self.ahead)
    }
}

#[test]
fn a_microphone_that_skips_audio_without_a_gap_shows_in_the_stats() {
    let air = Arc::new(Mutex::new(Air::default()));
    let (voice, mut mic, mut speaker) = open(Box::new(move |_, _| {
        let line = Line::new();
        let mic = Positioned { link: Room { air: air.clone(), line: line.clone() }, delivered: 0, ahead: 0, skip_at: Some(u64::from(RATE)) };
        let speaker = Positioned { link: Room { air: air.clone(), line }, delivered: 0, ahead: 0, skip_at: None };
        Ok(Opened { mic: Box::new(mic), speaker: Box::new(speaker), echo: EchoPath::InProcess })
    }));
    speaker.write(&vec![0.0; RATE as usize * 3]);
    voice.start();
    until(&voice, VoiceState::Running);
    let mut sink = vec![0.0; RATE as usize];
    quickly("the skip to show", || {
        mic.read(&mut sink);
        voice.stats().mic_device_ahead_frames == Some(480)
    });
    assert_eq!(voice.stats().speaker_device_ahead_frames, Some(0));
}
