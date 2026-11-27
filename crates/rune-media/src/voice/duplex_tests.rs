use super::duplex::{self, Heard, Render};
use super::echo_path::{EchoPath, SystemAec, choose};
use super::io::{self, Ends, Meters};
use super::{VoiceMic, VoiceSpeaker};
use std::collections::VecDeque;
use std::sync::Arc;

const RATE: u32 = 16_000;

struct Room {
    render: Render,
    heard: Heard,
    mic: VoiceMic,
    speaker: VoiceSpeaker,
    air: VecDeque<f32>,
    meters: Arc<Meters>,
    seed: u32,
}

impl Room {
    fn new(path: EchoPath, delay_ms: usize) -> Self {
        let Ends { capture, playback, mic, speaker, meters } = io::ends(RATE as usize * 20, RATE as usize * 20);
        let air = std::iter::repeat_n(0.0, RATE as usize * delay_ms / 1_000).collect();
        let (render, heard) = duplex::for_path(path, RATE, capture, playback);
        Self { render, heard, mic, speaker, air, meters, seed: 7 }
    }

    fn noise(&mut self) -> f32 {
        self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
    }

    fn speech(&mut self, at: usize) -> f32 {
        let t = at as f32 / RATE as f32;
        0.4 * (t * 3.0).sin().abs() * (self.noise() + 0.5 * (std::f32::consts::TAU * 330.0 * t).sin())
    }

    fn run(&mut self, frames: usize, block: usize, far: bool, near: bool) -> (f64, f64) {
        let (mut heard, mut kept) = (0.0, 0.0);
        let mut out = vec![0.0; block];
        let mut input = vec![0.0; block];
        let mut cleaned = vec![0.0; frames];
        for start in (0..frames).step_by(block) {
            if far {
                let talk: Vec<f32> = (start..start + block).map(|i| self.speech(i)).collect();
                self.speaker.write(&talk);
            }
            self.render.render(&mut out);
            for (slot, &played) in input.iter_mut().zip(&out) {
                self.air.push_back(played);
                let echo = 0.5 * self.air.pop_front().unwrap_or(0.0);
                let local = if near { 0.3 * self.noise() } else { 0.0 };
                *slot = echo + local + 0.0005 * self.noise();
            }
            heard += input.iter().map(|&s| f64::from(s * s)).sum::<f64>();
            self.heard.capture(&input);
            let n = self.mic.read(&mut cleaned);
            kept += cleaned[..n].iter().map(|&s| f64::from(s * s)).sum::<f64>();
        }
        (heard, kept)
    }
}

fn db(ratio: f64) -> f64 {
    10.0 * ratio.max(1e-20).log10()
}

#[test]
fn in_process_cancelling_removes_a_delayed_speaker_echo() {
    let mut room = Room::new(EchoPath::InProcess, 80);
    room.run(RATE as usize * 4, 160, true, false);
    let (heard, kept) = room.run(RATE as usize * 4, 160, true, false);
    let reduction = db(heard / kept);
    assert!(reduction >= 25.0, "echo reduced by only {reduction:.1} dB");
}

#[test]
fn a_speaker_thread_stall_that_starves_the_canceller_leaves_the_echo_cancelled() {
    let mut room = Room::new(EchoPath::InProcess, 80);
    let (block, period, burst) = (160, RATE as usize * 2, RATE as usize / 5);
    let talk: Vec<f32> = (0..period * 6).map(|i| if i % period < burst { room.speech(i) } else { 0.0 }).collect();
    room.speaker.write(&talk);
    let mut device: VecDeque<Vec<f32>> = VecDeque::new();
    let mut out = vec![0.0; block];
    let mut cleaned = Vec::new();
    let mut read = vec![0.0; RATE as usize];
    let mut silence = 0;
    for tick in 0..talk.len() / block {
        if !(650..658).contains(&tick) {
            room.render.silence(std::mem::take(&mut silence));
            while device.len() < 4 {
                room.render.render(&mut out);
                device.push_back(out.clone());
            }
        }
        let played = device.pop_front().unwrap_or_else(|| {
            silence += block as u64;
            vec![0.0; block]
        });
        let input: Vec<f32> = played
            .iter()
            .map(|&sample| {
                room.air.push_back(sample);
                0.5 * room.air.pop_front().unwrap_or(0.0) + 0.0005 * room.noise()
            })
            .collect();
        room.heard.capture(&input);
        let n = room.mic.read(&mut read);
        cleaned.extend_from_slice(&read[..n]);
    }
    let burst_energy: f64 = talk[..burst].iter().map(|&s| f64::from(s * s)).sum();
    let echo = |p: usize| db(cleaned[p * period..(p + 1) * period].iter().map(|&s| f64::from(s * s)).sum::<f64>() / burst_energy);
    let (before, after) = (echo(2), echo(4));
    assert_eq!(room.meters.stats().speaker_gap_frames, 5 * block as u64);
    assert!(after <= -40.0, "the burst after the stall came back at {after:.1} dB (before it, {before:.1} dB)");
}

#[test]
fn in_process_cancelling_keeps_the_near_talker() {
    let mut room = Room::new(EchoPath::InProcess, 80);
    room.run(RATE as usize * 2, 160, true, false);
    let (heard, kept) = room.run(RATE as usize * 2, 160, false, true);
    let change = db(kept / heard);
    assert!(change > -10.0, "near talk lost {change:.1} dB");
}

#[test]
fn a_system_canceller_is_never_stacked_with_the_in_process_one() {
    for system in [SystemAec { on: true, can_set: false }, SystemAec { on: false, can_set: true }] {
        let path = choose(Some(system));
        let mut room = Room::new(path, 0);
        let (heard, kept) = room.run(RATE as usize, 160, true, false);
        assert_eq!(heard, kept, "{system:?} chose {path:?}, which processed the mic");
    }
}

#[test]
fn without_a_usable_system_canceller_the_in_process_one_runs() {
    assert_eq!(choose(None), EchoPath::InProcess);
    assert_eq!(choose(Some(SystemAec { on: false, can_set: false })), EchoPath::InProcess);
}

#[test]
fn uneven_device_callbacks_reach_the_mic_in_ten_millisecond_blocks() {
    let mut room = Room::new(EchoPath::InProcess, 0);
    let mut fed = 0;
    for block in [7, 1_000, 3, 160, 481, 9] {
        room.heard.capture(&vec![0.0; block]);
        fed += block;
    }
    let ten_ms = RATE as usize / 100;
    assert_eq!(room.mic.available(), fed / ten_ms * ten_ms);
}

#[test]
fn the_canceller_reports_where_it_found_the_echo() {
    let mut room = Room::new(EchoPath::InProcess, 200);
    assert_eq!(room.meters.stats().echo_canceller, None);
    room.run(RATE as usize * 6, 160, true, false);
    let canceller = room.meters.stats().echo_canceller.expect("the in-app canceller reports once a second");
    let aligned = canceller.alignment.as_millis();
    assert!((180..=210).contains(&aligned), "aligned at {aligned} ms for a 200 ms echo");
    assert!(canceller.erle_db >= 10.0, "ERLE {:.1} dB", canceller.erle_db);
    let (low, high) = canceller.reference_lead_frames;
    assert!(high - low <= 160, "lead ranged {low}..{high} with render and capture in step");
}

#[test]
fn a_system_canceller_leaves_no_in_app_canceller_stats() {
    let mut room = Room::new(EchoPath::System, 80);
    room.run(RATE as usize * 2, 160, true, false);
    assert_eq!(room.meters.stats().echo_canceller, None);
}
