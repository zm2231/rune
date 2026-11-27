use super::{Links, MicLink, Opener, SpeakerLink, with_opener};
use crate::voice::machine::Fault;
use crate::voice::{Voice, VoiceEvent, VoiceSpec, VoiceState};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const RATE: u32 = 16_000;

#[derive(Default)]
struct Air {
    echo: VecDeque<f32>,
    heard_energy: Vec<f64>,
    seed: u32,
    fail_after: Option<usize>,
}

struct Room {
    air: Arc<Mutex<Air>>,
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
        let mut energy = 0.0;
        for slot in out.iter_mut() {
            air.seed = air.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let hiss = ((air.seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.001;
            *slot = 0.5 * air.echo.pop_front().unwrap_or(0.0) + hiss;
            energy += f64::from(*slot * *slot);
        }
        air.heard_energy.push(energy);
        Ok(())
    }
}

impl SpeakerLink for Room {
    fn write(&mut self, frames: &[f32]) -> Result<(), Fault> {
        let deadline = rune_core::time::now() + Duration::from_millis(20);
        while self.air.lock().unwrap().echo.len() > 4 * frames.len() && rune_core::time::now() < deadline {
            std::thread::sleep(Duration::from_micros(100));
        }
        self.air.lock().unwrap().echo.extend(frames);
        Ok(())
    }
}

fn opener(air: &Arc<Mutex<Air>>, mut failures: u32) -> Opener {
    let air = air.clone();
    Box::new(move |_, _| {
        if failures > 0 {
            failures -= 1;
            return Err(Fault::passing("connection refused"));
        }
        Ok((Box::new(Room { air: air.clone() }) as Box<dyn MicLink>, Box::new(Room { air: air.clone() }) as Box<dyn SpeakerLink>) as Links)
    })
}

fn open(open: Opener) -> (Voice, crate::voice::VoiceMic, crate::voice::VoiceSpeaker) {
    let spec = VoiceSpec::new(RATE).mic_buffer(Duration::from_secs(2)).speaker_buffer(Duration::from_secs(10));
    Voice::launch(spec, move |capture, playback, signals| with_opener(RATE, capture, playback, signals, open)).unwrap()
}

fn until(voice: &Voice, want: VoiceState) {
    let deadline = rune_core::time::now() + Duration::from_secs(10);
    while voice.state() != want {
        assert!(rune_core::time::now() < deadline, "stuck in {:?} waiting for {want:?}", voice.state());
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn speech(at: usize, seed: &mut u32) -> f32 {
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

fn energy(samples: &[f32]) -> f64 {
    samples.iter().map(|&s| f64::from(s * s)).sum()
}

fn db(ratio: f64) -> f64 {
    10.0 * ratio.max(1e-20).log10()
}

#[test]
#[ignore = "needs a sound server whose default source hears its default sink: scripts/voice-pulse-e2e.sh"]
fn a_real_sound_server_loop_is_cancelled() {
    let seconds = 8;
    let mut seed = 5;
    let talk: Vec<f32> = (0..RATE as usize * seconds).map(|i| speech(i, &mut seed)).collect();
    let block = RATE as usize / 100;
    let tail = talk.len() / 2..talk.len();

    let (mut raw_mic, mut raw_speaker) = super::pulse::open(RATE, block).expect("PulseAudio");
    let mut heard = vec![0.0; talk.len()];
    let silence = vec![0.0; block];
    raw_speaker.write(&silence).unwrap();
    raw_speaker.write(&silence).unwrap();
    for (out, mic) in talk.chunks(block).zip(heard.chunks_mut(block)) {
        raw_mic.read(mic).unwrap();
        raw_speaker.write(out).unwrap();
    }
    drop((raw_mic, raw_speaker));
    let off = db(energy(&heard[tail.clone()]) / energy(&talk[tail.clone()]));
    for second in 1..seconds {
        let at = second * RATE as usize;
        let window = &talk[at - RATE as usize / 2..at];
        let lag = (0..RATE as usize / 4).max_by(|&a, &b| {
            let score = |l: usize| window.iter().zip(&heard[at - RATE as usize / 2 + l..]).map(|(p, c)| p * c).sum::<f32>();
            score(a).total_cmp(&score(b))
        });
        println!("second {second}: loop delay {:.1} ms", lag.unwrap_or(0) as f32 * 1e3 / RATE as f32);
    }

    let spec = VoiceSpec::new(RATE).mic_buffer(Duration::from_secs(2)).speaker_buffer(Duration::from_secs(10));
    let (voice, mut mic, mut speaker) = Voice::open(spec).unwrap();
    speaker.write(&talk);
    voice.start();
    until(&voice, VoiceState::Running);
    let mut cleaned = Vec::with_capacity(talk.len());
    let mut buf = vec![0.0; 4_096];
    while cleaned.len() < talk.len() {
        let n = mic.read(&mut buf);
        cleaned.extend_from_slice(&buf[..n]);
        std::thread::sleep(Duration::from_millis(2));
    }
    let on = db(energy(&cleaned[tail.clone()]) / energy(&talk[tail]));
    println!("echo re speaker: AEC off {off:.1} dB, AEC on {on:.1} dB");
    assert!(off > -10.0, "the loop doesn't carry the speaker to the mic ({off:.1} dB), so it proves nothing");
    assert!(off - on >= 25.0, "echo reduced by only {:.1} dB", off - on);
}
