use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{Member, SharedState};
use crate::{Cx, El, Env, Runtime, el};

thread_local! {
    static BUILDS: Cell<u32> = const { Cell::new(0) };
}

#[derive(Default)]
struct Notes {
    count: u32,
    log: Vec<String>,
}

fn view(_: &Notes, _: &mut Cx<Notes>) -> El {
    BUILDS.with(|b| b.set(b.get() + 1));
    el().size(10.0, 10.0)
}

fn builds() -> u32 { BUILDS.with(Cell::get) }

fn window(shared: &SharedState<Notes>, key: &str) -> (Member<Notes>, Runtime<Notes>) {
    (shared.member(key), Runtime::new(shared.placeholder(), view, (10.0, 10.0), Env::new(true, 1.0)))
}

fn turn<S, R>(member: &mut Member<S>, rt: &mut Runtime<S>, f: impl FnOnce(&mut Runtime<S>) -> R) -> R {
    member.enter(rt);
    let result = f(rt);
    member.leave(rt);
    result
}

fn counter() -> (Arc<AtomicUsize>, crate::runtime::Waker) {
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    (count, Arc::new(move || { seen.fetch_add(1, Ordering::Relaxed); }))
}

#[test]
fn a_change_in_one_window_rebuilds_the_others_and_an_idle_turn_rebuilds_none() {
    let shared = SharedState::new(Notes::default());
    let (mut a, mut ra) = window(&shared, "a");
    let (mut b, mut rb) = window(&shared, "b");
    let ((a_woken, a_wake), (b_woken, b_wake)) = (counter(), counter());
    a.connect(a_wake);
    b.connect(b_wake);
    turn(&mut a, &mut ra, |rt| rt.frame(0.0));
    turn(&mut b, &mut rb, |rt| rt.frame(0.0));
    let before = builds();

    turn(&mut a, &mut ra, |rt| rt.update(|s, _| s.count = 1));
    assert_eq!((a_woken.load(Ordering::Relaxed), b_woken.load(Ordering::Relaxed)), (0, 1));
    turn(&mut a, &mut ra, |rt| rt.frame(0.016));
    assert_eq!(b_woken.load(Ordering::Relaxed), 1, "committing the change already reported wakes nobody again");
    turn(&mut b, &mut rb, |rt| {
        assert_eq!(rt.state.count, 1);
        rt.frame(0.016);
    });
    assert_eq!(builds(), before + 2);

    let idle = builds();
    turn(&mut a, &mut ra, |rt| rt.frame(0.032));
    turn(&mut b, &mut rb, |rt| rt.frame(0.032));
    assert_eq!(builds(), idle);
    assert_eq!((a_woken.load(Ordering::Relaxed), b_woken.load(Ordering::Relaxed)), (0, 1));
}

#[test]
fn a_live_only_handler_changes_nothing_shared() {
    let shared = SharedState::new(Notes::default());
    let (mut a, mut ra) = window(&shared, "a");
    let (mut b, mut rb) = window(&shared, "b");
    let (b_woken, b_wake) = counter();
    b.connect(b_wake);
    turn(&mut a, &mut ra, |rt| rt.frame(0.0));
    turn(&mut b, &mut rb, |rt| rt.frame(0.0));
    turn(&mut a, &mut ra, |rt| {
        rt.update(|_, tx| tx.live_only());
        rt.frame(0.016);
    });
    assert_eq!(b_woken.load(Ordering::Relaxed), 0);
}

#[test]
fn hooks_run_with_each_window_key_around_its_turn() {
    let shared = SharedState::new(Notes::default())
        .on_enter(|s, key| s.log.push(format!("enter {key}")))
        .on_leave(|s, key| s.log.push(format!("leave {key}")));
    let (mut a, mut ra) = window(&shared, "list");
    let (mut b, mut rb) = window(&shared, "note");
    turn(&mut a, &mut ra, |_| {});
    turn(&mut b, &mut rb, |rt| rt.state.log.push("note's turn".into()));
    let log = turn(&mut a, &mut ra, |rt| rt.state.log.clone());
    assert_eq!(log, ["enter list", "leave list", "enter note", "note's turn", "leave note", "enter list"]);
}

#[test]
fn hooks_that_swap_a_windows_own_fields_wake_no_other_window() {
    let shared = SharedState::new(Notes::default())
        .on_enter(|s, key| s.log.push(format!("enter {key}")))
        .on_leave(|s, key| s.log.push(format!("leave {key}")));
    let (mut a, mut ra) = window(&shared, "a");
    let (mut b, mut rb) = window(&shared, "b");
    let ((a_woken, a_wake), (b_woken, b_wake)) = (counter(), counter());
    a.connect(a_wake);
    b.connect(b_wake);
    for t in [0.0, 0.016, 0.032] {
        turn(&mut a, &mut ra, |rt| rt.frame(t));
        turn(&mut b, &mut rb, |rt| rt.frame(t));
    }
    assert_eq!((a_woken.load(Ordering::Relaxed), b_woken.load(Ordering::Relaxed)), (0, 0));
}

#[test]
#[should_panic(expected = "still holds their SharedState")]
fn a_turn_inside_another_windows_turn_panics() {
    let shared = SharedState::new(Notes::default());
    let (mut a, mut ra) = window(&shared, "a");
    let (mut b, mut rb) = window(&shared, "b");
    a.enter(&mut ra);
    b.enter(&mut rb);
}

struct Tracked(Option<Rc<Cell<u32>>>);

impl Drop for Tracked {
    fn drop(&mut self) {
        if let Some(drops) = &self.0 { drops.set(drops.get() + 1); }
    }
}

fn tracked_view(_: &Tracked, _: &mut Cx<Tracked>) -> El { el() }

#[test]
fn the_state_drops_once_when_a_window_goes_away_inside_its_turn() {
    let drops = Rc::new(Cell::new(0));
    let shared = SharedState::with_placeholder(Tracked(Some(drops.clone())), || Tracked(None));
    let mut a = shared.member("a");
    let mut ra = Runtime::new(shared.placeholder(), tracked_view, (10.0, 10.0), Env::default());
    a.enter(&mut ra);
    a.release(&mut ra);
    drop((a, ra));
    assert_eq!(drops.get(), 0);
    drop(shared);
    assert_eq!(drops.get(), 1);
}

struct Big { bytes: [u8; 1 << 20], count: u32 }

fn big() -> Big { Big { bytes: [0; 1 << 20], count: 0 } }

fn big_view(_: &Big, _: &mut Cx<Big>) -> El { el() }

fn counted<R>(allocs: &mut usize, f: impl FnOnce() -> R) -> R {
    let before = crate::tooling::test_alloc::allocs();
    let result = f();
    *allocs += crate::tooling::test_alloc::allocs() - before;
    result
}

#[test]
fn a_turn_neither_moves_a_large_state_nor_allocates() {
    std::thread::Builder::new().stack_size(64 << 20).spawn(|| {
        let shared = SharedState::with_placeholder(big(), big);
        let (mut a, mut ra) = (shared.member("a"), Runtime::new(big(), big_view, (10.0, 10.0), Env::default()));
        let (mut b, mut rb) = (shared.member("b"), Runtime::new(big(), big_view, (10.0, 10.0), Env::default()));
        let ((_, a_wake), (b_woken, b_wake)) = (counter(), counter());
        a.connect(a_wake);
        b.connect(b_wake);
        a.enter(&mut ra);
        let home: *const Big = &*ra.state;
        a.leave(&mut ra);
        let mut allocs = 0;
        for n in 1..=4 {
            counted(&mut allocs, || a.enter(&mut ra));
            assert!(std::ptr::eq(&*ra.state, home), "the state stays where it is");
            ra.update(|s, _| { s.count = n; s.bytes[(1 << 20) - 1] = n as u8; });
            counted(&mut allocs, || a.leave(&mut ra));
            counted(&mut allocs, || b.enter(&mut rb));
            assert!(std::ptr::eq(&*rb.state, home), "the other window sees the same state, not a copy");
            assert_eq!((rb.state.count, rb.state.bytes[(1 << 20) - 1]), (n, n as u8));
            counted(&mut allocs, || b.leave(&mut rb));
        }
        assert_eq!(b_woken.load(Ordering::Relaxed), 4, "each change still wakes the other window");
        assert_eq!(allocs, 0, "entering and leaving allocate nothing");
    }).unwrap().join().unwrap();
}
