use super::{FakeVoice, Voice, VoiceError, VoiceEvent, VoiceMic, VoiceSpeaker, VoiceSpec, VoiceState};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const RATE: u32 = 16_000;
const BLOCK: usize = 160;

fn until(voice: &Voice, want: VoiceState) {
    let deadline = rune_core::time::now() + Duration::from_secs(5);
    while voice.state() != want {
        assert!(rune_core::time::now() < deadline, "stuck in {:?} waiting for {want:?}", voice.state());
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn running(fake: &FakeVoice) -> (Voice, VoiceMic, VoiceSpeaker) {
    let (voice, mic, speaker) = Voice::open_fake(VoiceSpec::new(RATE), fake).unwrap();
    voice.start();
    until(&voice, VoiceState::Running);
    (voice, mic, speaker)
}

fn read_all(mic: &mut VoiceMic) -> Vec<f32> {
    let mut out = vec![0.0; mic.available()];
    let n = mic.read(&mut out);
    out.truncate(n);
    out
}

#[test]
fn the_fake_mic_hears_the_speaker_at_the_echo_gain_and_delay() {
    let fake = FakeVoice::new().echo(0.5, Duration::from_millis(10));
    let (voice, mut mic, mut speaker) = running(&fake);
    let ramp: Vec<f32> = (0..BLOCK).map(|i| i as f32 / BLOCK as f32).collect();
    speaker.write(&ramp);
    assert!(fake.tick(BLOCK));
    assert!(fake.tick(BLOCK));
    let heard = read_all(&mut mic);
    assert_eq!(heard.len(), 2 * BLOCK);
    assert!(heard[..BLOCK].iter().all(|&s| s == 0.0));
    let expected: Vec<f32> = ramp.iter().map(|s| s * 0.5).collect();
    assert_eq!(&heard[BLOCK..], &expected[..]);
    assert_eq!(voice.stats().captured_frames, 2 * BLOCK as u64);
}

#[test]
fn near_speech_reaches_the_mic_alongside_the_echo() {
    let fake = FakeVoice::new().echo(0.25, Duration::ZERO);
    let (_voice, mut mic, mut speaker) = running(&fake);
    fake.say(&[0.5; BLOCK]);
    speaker.write(&[1.0; BLOCK]);
    fake.tick(BLOCK);
    assert!(read_all(&mut mic).iter().all(|&s| s == 0.75));
}

#[test]
fn clear_mid_stream_silences_the_next_callback_and_freezes_played_frames() {
    let fake = FakeVoice::new();
    let (_voice, _mic, mut speaker) = running(&fake);
    speaker.write(&[1.0; 10 * BLOCK]);
    fake.tick(BLOCK);
    assert_eq!(fake.last_played(), vec![1.0; BLOCK]);
    assert_eq!(speaker.played_frames(), BLOCK as u64);

    speaker.clear();
    fake.tick(BLOCK);
    assert_eq!(fake.last_played(), vec![0.0; BLOCK]);
    assert_eq!(speaker.played_frames(), BLOCK as u64);
    assert_eq!(speaker.queued(), 0);

    speaker.write(&[0.5; BLOCK]);
    fake.tick(BLOCK);
    assert_eq!(fake.last_played(), vec![0.5; BLOCK]);
    assert_eq!(speaker.played_frames(), 2 * BLOCK as u64);
}

#[test]
fn mute_silences_the_mic_and_leaves_the_speaker_playing() {
    let fake = FakeVoice::new();
    let (voice, mut mic, mut speaker) = running(&fake);
    voice.set_muted(true);
    let deadline = rune_core::time::now() + Duration::from_secs(5);
    while !voice.muted() {
        assert!(rune_core::time::now() < deadline);
        std::thread::yield_now();
    }
    fake.say(&[0.9; BLOCK]);
    speaker.write(&[0.3; BLOCK]);
    fake.tick(BLOCK);
    assert!(read_all(&mut mic).iter().all(|&s| s == 0.0));
    assert_eq!(fake.last_played(), vec![0.3; BLOCK]);
}

#[test]
fn a_device_change_rebuilds_the_stream_and_keeps_queued_audio() {
    let fake = FakeVoice::new();
    let (voice, _mic, mut speaker) = running(&fake);
    assert_eq!(voice.take_events(), vec![VoiceEvent::Started]);
    speaker.write(&[0.7; BLOCK]);
    let discards = fake.lock().discards;
    fake.change_devices();
    until(&voice, VoiceState::Reconnecting);
    until(&voice, VoiceState::Running);
    assert_eq!(voice.take_events(), vec![VoiceEvent::DeviceChanged]);
    assert!(fake.lock().discards > discards);
    fake.tick(BLOCK);
    assert_eq!(fake.last_played(), vec![0.7; BLOCK]);
}

#[test]
fn an_interruption_pauses_the_stream_and_its_end_resumes_it() {
    let fake = FakeVoice::new();
    let (voice, _mic, _speaker) = running(&fake);
    voice.take_events();
    fake.interrupt();
    until(&voice, VoiceState::Interrupted);
    assert!(!fake.tick(BLOCK));
    fake.end_interruption(true);
    until(&voice, VoiceState::Running);
    assert_eq!(voice.take_events(), vec![VoiceEvent::Interrupted, VoiceEvent::Resumed]);
    assert!(fake.tick(BLOCK));
}

#[test]
fn the_background_keeps_a_call_rendering() {
    let fake = FakeVoice::new();
    let (voice, _mic, _speaker) = running(&fake);
    voice.take_events();
    fake.background();
    voice.set_muted(true);
    let deadline = rune_core::time::now() + Duration::from_secs(5);
    while !voice.muted() {
        assert!(rune_core::time::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(voice.state(), VoiceState::Running);
    assert!(voice.take_events().is_empty());
    assert!(fake.tick(BLOCK));
}

#[test]
fn a_media_reset_rebuilds_a_running_stream() {
    let fake = FakeVoice::new();
    let (voice, _mic, _speaker) = running(&fake);
    voice.take_events();
    fake.reset_media();
    until(&voice, VoiceState::Reconnecting);
    until(&voice, VoiceState::Running);
    assert_eq!(voice.take_events(), vec![VoiceEvent::DeviceChanged]);
}

#[test]
fn a_device_that_fails_to_start_is_retried() {
    let fake = FakeVoice::new();
    fake.lock().failing_runs = 2;
    let (voice, _mic, _speaker) = running(&fake);
    assert_eq!(fake.lock().runs, 3);
    assert_eq!(voice.take_events(), vec![VoiceEvent::Started]);
}

#[test]
fn stop_and_drop_stop_the_device() {
    let fake = FakeVoice::new();
    let (voice, _mic, _speaker) = running(&fake);
    voice.stop();
    until(&voice, VoiceState::Idle);
    assert!(!fake.tick(BLOCK));
    voice.start();
    until(&voice, VoiceState::Running);
    drop(voice);
    assert!(!fake.tick(BLOCK));
}

#[test]
fn the_waker_hears_state_changes() {
    let fake = FakeVoice::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    let spec = VoiceSpec::new(RATE).waker(Arc::new(move || {
        counted.fetch_add(1, Ordering::SeqCst);
    }));
    let (voice, _mic, _speaker) = Voice::open_fake(spec, &fake).unwrap();
    voice.start();
    until(&voice, VoiceState::Running);
    assert!(calls.load(Ordering::SeqCst) >= 1);
}

#[test]
fn an_oversized_callback_is_counted_and_moves_no_audio() {
    let fake = FakeVoice::new();
    let (voice, mut mic, _speaker) = running(&fake);
    assert!(fake.tick(super::fake::MAX_TICK + 1));
    assert_eq!(mic.available(), 0);
    assert_eq!(voice.stats().oversized_callbacks, 1);
    assert_eq!(read_all(&mut mic).len(), 0);
}

#[test]
fn an_out_of_range_rate_is_refused() {
    let fake = FakeVoice::new();
    assert!(matches!(Voice::open_fake(VoiceSpec::new(96_000), &fake), Err(VoiceError::InvalidSpec(_))));
    assert!(matches!(Voice::open_fake(VoiceSpec::new(4_000), &fake), Err(VoiceError::InvalidSpec(_))));
}

#[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "linux")))]
#[test]
fn platforms_without_voice_io_say_so() {
    assert_eq!(Voice::open(VoiceSpec::new(RATE)).err(), Some(VoiceError::Unsupported));
    assert_eq!(Voice::permission(), crate::recorder::MicPermission::Restricted);
}
