use super::{CapturePosition, Packet};

const RATE: u32 = 48_000;

fn packet(position: u64, discontinuity: bool) -> Packet {
    Packet { position, frames: 480, discontinuity, timed: true }
}

#[test]
fn a_reported_gap_is_filled_up_to_a_second_and_never_reads_as_a_skip() {
    let mut capture = CapturePosition::new(RATE);
    assert_eq!(capture.packet(&packet(1_000, false)), 0);
    let steady = capture.ahead();
    assert_eq!(capture.packet(&packet(1_480 + 4_753, true)), 4_753);
    assert_eq!(capture.ahead(), steady);
    assert_eq!(capture.packet(&packet(6_713 + 2 * u64::from(RATE), true)), u64::from(RATE));
    assert_eq!(capture.ahead(), steady);
}

#[test]
fn a_position_jump_without_a_reported_gap_reads_as_a_skip() {
    let mut capture = CapturePosition::new(RATE);
    capture.packet(&packet(0, false));
    capture.packet(&packet(480, false));
    let steady = capture.ahead().unwrap();
    assert_eq!(capture.packet(&packet(960 + 960, false)), 0);
    assert_eq!(capture.ahead(), Some(steady + 960));
}

#[test]
fn a_packet_without_a_timestamp_leaves_the_position_unknown_until_the_next() {
    let mut capture = CapturePosition::new(RATE);
    capture.packet(&packet(0, false));
    let steady = capture.ahead();
    capture.packet(&Packet { position: 0, frames: 480, discontinuity: false, timed: false });
    assert_eq!(capture.ahead(), None);
    capture.packet(&packet(960, false));
    assert_eq!(capture.ahead(), steady);
}
