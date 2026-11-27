use crate::voice::streams::Opened;
use crate::voice::streams_tests::{RATE, speech, until};
use crate::voice::{Voice, VoiceSpec, VoiceState};
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
