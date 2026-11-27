use super::*;
use crate::{Cx, El, Env, ScrollSpec, col, el, oklch, row, text};
use std::time::Duration;

const STOPS: &[(f32, f32)] = &[(0.0, 1.0), (0.1, 1.0), (0.5, 0.2), (1.0, 0.2)];
const FRAME: f64 = 1.0 / 60.0;
const TIMER: Duration = Duration::from_millis(250);

#[derive(Default)]
struct Board {
    polls: u32,
    change_at: u32,
}

fn spinner(k: usize) -> El {
    let mut e = el().role("progressbar").label("Loading").size(16.0, 16.0).pixel_snap(false);
    for d in 0..24 {
        let (x, y) = ((d % 5) as f32 * 3.0, (d / 5) as f32 * 3.0);
        let c = crate::Cycle { period: 1.8, phase: d as f32 / 24.0, stops: STOPS, still: 0.5, fps: 60.0 };
        e = e.child(el().key(format!("s{k}d{d}")).absolute(x, y).size(2.0, 2.0).radius(0.5).fill(oklch(0.6, 0.1, 250.0)).a11y_hidden().cycle(c));
    }
    e
}

fn board(_: &Board, _: &mut Cx<Board>) -> El {
    let rows = (0..102).map(|i| {
        let r = row().key(format!("row{i}")).w_full().h(28.0).gap(6.0).child(text(format!("Row {i} waiting on you")));
        let r = if i < 6 { r.child(spinner(i)) } else { r };
        r.child(el().key(format!("open{i}")).size(40.0, 20.0).fill(oklch(0.5, 0.05, 200.0)).label("Open").on_press(|_: &mut Board| {}))
    });
    col().key("list").w_full().h_full().scroll(ScrollSpec::default()).children(rows)
}

fn timer_poll(s: &mut Board, _: f64, tx: &mut Tx) -> bool {
    s.polls += 1;
    tx.wake_after(TIMER);
    s.polls == s.change_at
}

fn wake_poll(s: &mut Board, _: f64, _: &mut Tx) -> bool {
    s.polls += 1;
    false
}

fn window(poll: fn(&mut Board, f64, &mut Tx) -> bool) -> (Runtime<Board>, crate::AccessibilityLink) {
    let mut rt = Runtime::new(Board::default(), board, (500.0, 900.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.wall_start = Some(crate::time::now());
    rt.frame.set_test_clock(crate::ManualClock::new(60.0));
    rt.frame(0.0);
    rt.frame(crate::frame::clock::RESUME_STEP);
    let mut link = crate::AccessibilityLink::new();
    link.activate();
    assert!(link.service(&mut rt, "Board").is_some_and(|u| u.tree.is_some()));
    (rt, link)
}

#[derive(Default, Debug)]
struct Count {
    frames: u32,
    polls: u32,
    paints: u64,
    commits: u64,
    a11y_updates: u32,
    allocations: usize,
}

fn run(rt: &mut Runtime<Board>, link: &mut crate::AccessibilityLink, frames: u32, wake_every: Option<u32>) -> Count {
    let (paints, commits, polls) = (rt.paint_count(), rt.stats.commits, rt.state.polls);
    let mut count = Count { frames, ..Count::default() };
    for f in 0..frames {
        let clock = rt.frame.test_clock_mut().unwrap();
        clock.seek(clock.now() + FRAME);
        let woke = wake_every.is_some_and(|n| f % n == n - 1);
        let allocs = crate::tooling::test_alloc::allocs();
        if rt.loop_step_polled(FRAME, woke).is_err() {
            rt.frame(FRAME);
        }
        if link.service(rt, "Board").is_some() { count.a11y_updates += 1; }
        count.allocations += crate::tooling::test_alloc::allocs() - allocs;
    }
    count.polls = rt.state.polls - polls;
    count.paints = rt.paint_count() - paints;
    count.commits = rt.stats.commits - commits;
    count
}

#[test]
fn a_due_timer_whose_poll_changes_nothing_steps_the_spinners_alone() {
    let (mut rt, mut link) = window(timer_poll);
    let count = run(&mut rt, &mut link, 120, None);
    assert!((7..=8).contains(&count.polls), "the 250 ms timer polls every quarter second: {count:?}");
    assert_eq!((count.paints, count.commits, count.a11y_updates), (0, 0, 0), "{count:?}");
    assert!(count.allocations <= count.frames as usize * 8, "{count:?}");
}

#[test]
fn an_app_wake_whose_poll_changes_nothing_steps_the_spinners_alone() {
    let (mut rt, mut link) = window(wake_poll);
    let count = run(&mut rt, &mut link, 120, Some(10));
    assert_eq!(count.polls, 12, "each wake polls once: {count:?}");
    assert_eq!((count.paints, count.commits, count.a11y_updates), (0, 0, 0), "{count:?}");
}

#[test]
fn a_poll_that_changes_something_gets_one_ordinary_frame_and_polls_once() {
    let (mut rt, mut link) = window(timer_poll);
    rt.state.change_at = rt.state.polls + 3;
    let count = run(&mut rt, &mut link, 60, None);
    assert_eq!((count.commits, count.paints), (1, 1), "{count:?}");
    assert!((3..=4).contains(&count.polls), "one poll a timer; the frame doesn't poll again: {count:?}");
}
