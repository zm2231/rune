#![allow(unsafe_code)]

use rune_media::{FakeVoice, Voice, VoiceSpec, VoiceState};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::time::Duration;

struct Counting;
thread_local! { static ALLOCS: Cell<usize> = const { Cell::new(0) }; }
fn bump() { ALLOCS.try_with(|c| c.set(c.get() + 1)).ok(); }

// SAFETY: forwards every allocation call to `System`, only counting allocations on this thread.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        bump();
        // SAFETY: delegates allocation with the caller's layout to `System`.
        unsafe { System.alloc(l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        bump();
        // SAFETY: delegates zeroed allocation with the caller's layout to `System`.
        unsafe { System.alloc_zeroed(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        // SAFETY: delegates deallocation of `p` and `l` to `System`.
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        bump();
        // SAFETY: delegates reallocation of `p`, `l` and `n` to `System`.
        unsafe { System.realloc(p, l, n) }
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

fn allocs() -> usize {
    ALLOCS.with(Cell::get)
}

#[test]
fn audio_callbacks_and_stream_ends_allocate_nothing() {
    let fake = FakeVoice::new().echo(0.5, Duration::from_millis(20));
    let (voice, mut mic, mut speaker) = Voice::open_fake(VoiceSpec::new(48_000), &fake).unwrap();
    voice.start();
    let deadline = rune_core::time::now() + Duration::from_secs(5);
    while voice.state() != VoiceState::Running {
        assert!(rune_core::time::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    let tone = vec![0.25f32; 480];
    let mut heard = vec![0.0f32; 480];
    fake.tick(480);

    let before = allocs();
    for i in 0..10_000 {
        speaker.write(&tone);
        if i % 97 == 0 {
            speaker.clear();
        }
        fake.tick(480);
        mic.read(&mut heard);
        let _ = (speaker.queued(), speaker.played_frames(), mic.available(), voice.stats());
    }
    let delta = allocs() - before;
    assert_eq!(delta, 0, "10,000 voice callbacks allocated {delta} times");
}
