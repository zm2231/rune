use crate::voice::machine::Fault;
use crate::voice::streams::{self, Opened, SpeakerLink};
use crate::voice::streams_tests::{RATE, speech, until};
use crate::voice::{Voice, VoiceSpec, VoiceState};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

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

    let Opened { mic: mut raw_mic, speaker: mut raw_speaker, .. } = super::pulse::open(RATE, block).expect("PulseAudio");
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
        let lag = (0..RATE as usize).max_by(|&a, &b| {
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

const STALL_AT: usize = 450;
const STALL: Duration = Duration::from_millis(80);

struct StallingSpeaker {
    speaker: Box<dyn SpeakerLink>,
    written: usize,
    after_stall: Arc<AtomicU64>,
}

impl SpeakerLink for StallingSpeaker {
    fn begin(&mut self) {
        self.speaker.begin();
    }

    fn write(&mut self, frames: &[f32]) -> Result<(), Fault> {
        self.written += 1;
        if self.written == STALL_AT {
            std::thread::sleep(STALL);
        }
        self.speaker.write(frames)
    }

    fn take_silence(&mut self) -> u64 {
        let silence = self.speaker.take_silence();
        if self.written >= STALL_AT {
            self.after_stall.fetch_add(silence, Ordering::Relaxed);
        }
        silence
    }
}

fn bursts_through_a_speaker_stall(open: fn(u32, usize) -> Result<Opened, String>) -> (u64, f64) {
    let period = RATE as usize;
    let burst = RATE as usize / 10;
    let seconds = 10;
    let mut seed = 9;
    let talk: Vec<f32> = (0..period * seconds).map(|i| if i % period < burst { speech(i, &mut seed) } else { 0.0 }).collect();
    let after_stall = Arc::new(AtomicU64::new(0));
    let counted = after_stall.clone();
    let open = Box::new(move |rate, block| {
        let mut opened = open(rate, block).map_err(Fault::passing)?;
        opened.speaker = Box::new(StallingSpeaker { speaker: opened.speaker, written: 0, after_stall: counted.clone() });
        Ok(opened)
    });
    let spec = VoiceSpec::new(RATE).mic_buffer(Duration::from_secs(2)).speaker_buffer(Duration::from_secs(12));
    let (voice, mut mic, mut speaker) = Voice::launch(spec, move |capture, playback, signals| streams::device(RATE, capture, playback, signals, open)).unwrap();
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
    let stats = voice.stats();
    drop(voice);
    let after_stall = after_stall.load(Ordering::Relaxed);
    assert!(stats.speaker_gap_frames >= after_stall, "the stats missed gaps the device reported");
    let echoes: Vec<f64> = (2..seconds - 1).map(|p| db(energy(&cleaned[p * period..p * period + period / 2]) / energy(&talk[p * period..p * period + burst]))).collect();
    let ms = |frames: u64| frames as f64 * 1e3 / f64::from(RATE);
    println!(
        "speaker gaps {:.1} ms, {:.1} ms of them after the stall, echo per burst re burst: {}",
        ms(stats.speaker_gap_frames),
        ms(after_stall),
        echoes.iter().map(|e| format!("{e:.1}")).collect::<Vec<_>>().join(" ")
    );
    (after_stall, echoes.iter().copied().fold(f64::MIN, f64::max))
}

fn alsa(rate: u32, _block: usize) -> Result<Opened, String> {
    super::alsa::open(rate)
}

#[test]
#[ignore = "needs a sound server whose default source hears its default sink: scripts/voice-pulse-e2e.sh"]
fn a_pulseaudio_speaker_underrun_is_counted_and_the_echo_stays_cancelled() {
    let (after_stall, worst) = bursts_through_a_speaker_stall(super::pulse::open);
    assert!(after_stall > 0, "the stall didn't make the speaker run dry, so it proves nothing");
    assert!(worst <= -25.0, "a burst came back at {worst:.1} dB");
}

#[test]
#[ignore = "needs ALSA's default device routed to a sound server that hears itself: scripts/voice-pulse-e2e.sh"]
fn an_alsa_speaker_xrun_is_counted_and_the_echo_stays_cancelled() {
    let (after_stall, worst) = bursts_through_a_speaker_stall(alsa);
    assert!(after_stall > 0, "the stall didn't make the speaker run dry, so it proves nothing");
    assert!(worst <= -25.0, "a burst came back at {worst:.1} dB");
}
