use super::io::ends;
use std::sync::atomic::Ordering;

#[test]
fn the_mic_delivers_frames_in_order_and_drops_what_does_not_fit() {
    let mut e = ends(4, 4);
    e.capture.capture(&[1.0, 2.0, 3.0]);
    e.capture.capture(&[4.0, 5.0, 6.0]);
    let mut out = [0.0; 8];
    assert_eq!(e.mic.read(&mut out), 4);
    assert_eq!(&out[..4], &[1.0, 2.0, 3.0, 4.0]);
    assert_eq!(e.mic.dropped_frames(), 2);
    assert_eq!(e.meters.stats().captured_frames, 6);
}

#[test]
fn a_muted_mic_delivers_silence_of_the_same_length() {
    let mut e = ends(8, 8);
    e.meters.muted.store(true, Ordering::Relaxed);
    e.capture.capture(&[0.5; 3]);
    let mut out = [9.0; 3];
    assert_eq!(e.mic.read(&mut out), 3);
    assert_eq!(out, [0.0; 3]);
}

#[test]
fn the_speaker_plays_queued_frames_then_silence() {
    let mut e = ends(8, 8);
    assert_eq!(e.speaker.write(&[0.1, 0.2, 0.3]), 3);
    let mut out = [9.0; 5];
    e.playback.render(&mut out);
    assert_eq!(out, [0.1, 0.2, 0.3, 0.0, 0.0]);
    assert_eq!(e.speaker.played_frames(), 3);
    assert_eq!(e.speaker.queued(), 0);
}

#[test]
fn a_full_speaker_takes_only_what_fits() {
    let mut e = ends(8, 4);
    assert_eq!(e.speaker.write(&[1.0; 6]), 4);
    assert_eq!(e.speaker.free(), 0);
}

#[test]
fn clear_drops_queued_frames_but_not_later_ones() {
    let mut e = ends(8, 16);
    e.speaker.write(&[1.0; 10]);
    let mut out = [0.0; 2];
    e.playback.render(&mut out);
    e.speaker.clear();
    e.speaker.write(&[0.5; 3]);
    let mut out = [9.0; 4];
    e.playback.render(&mut out);
    assert_eq!(out, [0.5, 0.5, 0.5, 0.0]);
    assert_eq!(e.speaker.played_frames(), 5);
}

#[test]
fn clear_twice_before_a_callback_drops_both_batches() {
    let mut e = ends(8, 16);
    e.speaker.write(&[1.0; 4]);
    e.speaker.clear();
    e.speaker.write(&[2.0; 4]);
    e.speaker.clear();
    let mut out = [9.0; 4];
    e.playback.render(&mut out);
    assert_eq!(out, [0.0; 4]);
    assert_eq!(e.speaker.played_frames(), 0);
}

#[test]
fn start_to_audio_is_measured_from_start_to_the_first_callback() {
    let mut e = ends(8, 8);
    e.playback.render(&mut [0.0; 2]);
    assert_eq!(e.meters.stats().start_to_audio, None);
    e.meters.mark_start();
    assert_eq!(e.meters.stats().start_to_audio, None);
    e.playback.render(&mut [0.0; 2]);
    assert!(e.meters.stats().start_to_audio.is_some());
}
