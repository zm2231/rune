use super::{Audio, CaptureQueue, Packet};

const RATE: u32 = 48_000;
const BLOCK: usize = 480;

fn tone(at: usize) -> Vec<f32> {
    (0..BLOCK).map(|i| ((at + i) as f32 * 0.01).sin() * 0.5 + 0.6).collect()
}

fn packet(queue: &mut CaptureQueue, position: u64, samples: &[f32]) {
    queue.packet(&Packet { position, timed: true, audio: Audio::Samples(samples) });
}

fn drain(queue: &mut CaptureQueue) -> Vec<f32> {
    let mut all = Vec::new();
    let mut out = vec![0.0; BLOCK];
    while queue.read(&mut out) {
        all.extend_from_slice(&out);
    }
    all
}

#[test]
fn a_skip_is_filled_with_silence_before_the_next_packet_whether_or_not_it_was_flagged() {
    let mut queue = CaptureQueue::new(RATE);
    let (first, second) = (tone(0), tone(BLOCK));
    packet(&mut queue, 1_000, &first);
    let steady = queue.ahead();
    packet(&mut queue, 1_000 + 480 + 960, &second);
    assert_eq!(queue.take_lost(), 960);
    assert_eq!(queue.ahead(), steady);
    let heard = drain(&mut queue);
    assert_eq!(heard, [first, vec![0.0; 960], second].concat());
}

#[test]
fn a_skip_over_a_second_is_filled_with_exactly_a_second_and_keeps_the_audio_around_it() {
    let mut queue = CaptureQueue::new(RATE);
    let (first, second) = (tone(0), tone(BLOCK));
    packet(&mut queue, 0, &first);
    let steady = queue.ahead();
    packet(&mut queue, 480 + 3 * u64::from(RATE), &second);
    assert_eq!(queue.take_lost(), u64::from(RATE));
    assert_eq!(queue.ahead(), steady);
    let heard = drain(&mut queue);
    assert_eq!(heard, [first, vec![0.0; RATE as usize], second].concat());
}

#[test]
fn a_position_that_moves_back_fills_nothing_and_reads_as_a_step() {
    let mut queue = CaptureQueue::new(RATE);
    packet(&mut queue, 4_800, &tone(0));
    let steady = queue.ahead().unwrap();
    packet(&mut queue, 4_800, &tone(BLOCK));
    assert_eq!(queue.take_lost(), 0);
    assert_eq!(queue.ahead(), Some(steady - 480));
    assert_eq!(drain(&mut queue).len(), 2 * BLOCK);
}

#[test]
fn a_packet_without_a_timestamp_leaves_the_position_unknown_until_the_next() {
    let mut queue = CaptureQueue::new(RATE);
    packet(&mut queue, 0, &tone(0));
    let steady = queue.ahead();
    queue.packet(&Packet { position: 0, timed: false, audio: Audio::Silent(BLOCK) });
    assert_eq!(queue.ahead(), None);
    packet(&mut queue, 960, &tone(BLOCK));
    assert_eq!(queue.take_lost(), 0);
    assert_eq!(queue.ahead(), steady);
}
