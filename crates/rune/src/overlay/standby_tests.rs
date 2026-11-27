use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::*;
use crate::overlay::OverlayHandle;
use crate::runtime::Env;
use crate::{Cx, El, el};

#[derive(Default)]
struct State {
    runs: Vec<&'static str>,
    read: Option<String>,
}

fn view(_: &State, _: &mut Cx<State>) -> El { el() }

fn poll(s: &mut State, _: f64, _: &mut crate::Tx) -> bool {
    s.runs.push("poll");
    false
}

fn overlay() -> (Standby, OverlayHandle<State>, Unmounted<State>) {
    let shared = Arc::new(Shared::default());
    let mut rt = Runtime::new(State::default(), view, (40.0, 20.0), Env::new(false, 1.0));
    rt.wall_start = Some(crate::time::now());
    let parked = Rc::new(RefCell::new(Some(Parked { rt, clipboard: Client::default() })));
    (Standby::new(shared.clone(), parked.clone()), OverlayHandle { shared }, parked)
}

fn runs(parked: &Unmounted<State>) -> Vec<&'static str> {
    parked.borrow().as_ref().map(|p| p.rt.state.runs.clone()).unwrap_or_default()
}

fn ran(parked: &Unmounted<State>, what: &str) -> bool { runs(parked).contains(&what) }

fn polls(parked: &Unmounted<State>) -> usize { runs(parked).iter().filter(|run| **run == "poll").count() }

fn turn(standby: &mut Standby, hidden: bool, mounted: Option<Mounted<'_>>) -> (bool, Option<(Attempt, f64)>) {
    standby.new_turn();
    standby.queued(hidden);
    if !hidden { return (false, None) }
    let run = standby.end_of_turn().run;
    (run, standby.step(run, mounted))
}

#[test]
fn a_callback_asked_while_shown_runs_on_the_turn_after_the_hide_and_not_before() {
    let (mut standby, handle, parked) = overlay();
    standby.shown();
    handle.after_hidden(|s, _| s.runs.push("picked"));
    assert!(!turn(&mut standby, false, None).0);
    assert!(standby.hidden().wake, "the hide asks for another turn");
    assert!(!standby.end_of_turn().run, "nothing runs in the turn the overlay hid");
    assert!(!ran(&parked, "picked"));
    assert!(turn(&mut standby, true, None).0);
    assert!(ran(&parked, "picked"));
    assert!(!turn(&mut standby, true, None).0, "a callback runs once");
}

#[test]
fn a_callback_asked_while_hidden_runs_on_the_next_turn() {
    let (mut standby, handle, parked) = overlay();
    handle.after_hidden(|s, _| s.runs.push("late"));
    assert!(!turn(&mut standby, true, None).0);
    assert!(turn(&mut standby, true, None).0);
    assert!(ran(&parked, "late"));
}

#[test]
fn showing_again_before_the_next_turn_keeps_the_callback_for_the_next_hide() {
    let (mut standby, handle, parked) = overlay();
    standby.shown();
    handle.after_hidden(|s, _| s.runs.push("picked"));
    turn(&mut standby, false, None);
    standby.hidden();
    standby.shown();
    turn(&mut standby, false, None);
    standby.hidden();
    assert!(!standby.end_of_turn().run, "not in the turn of the second hide");
    assert!(!ran(&parked, "picked"));
    turn(&mut standby, true, None);
    assert!(ran(&parked, "picked"));
}

#[test]
fn updates_reach_a_hidden_overlay_without_a_frame_whether_or_not_they_report_a_change() {
    let (mut standby, handle, parked) = overlay();
    handle.update(|s, _| s.runs.push("update"));
    handle.update_if(|s, _| { s.runs.push("update_if"); false });
    assert!(standby.has_updates());
    turn(&mut standby, true, None);
    assert!(!standby.has_updates());
    assert_eq!(runs(&parked), ["update", "update_if"]);
}

#[test]
fn wake_after_runs_the_hidden_overlays_poll_at_its_deadline() {
    let (mut standby, handle, parked) = overlay();
    if let Some(p) = parked.borrow_mut().as_mut() { p.rt.poll = Some(poll); }
    handle.update(|_, tx| tx.wake_after(Duration::from_millis(20)));
    let (_, timer) = turn(&mut standby, true, None);
    let (attempt, seconds) = timer.expect("the deadline becomes a timer");
    assert!(seconds > 0.0 && seconds <= 0.02, "{seconds}");
    let before = polls(&parked);
    std::thread::sleep(Duration::from_millis(30));
    assert!(standby.timer(attempt));
    standby.step(false, None);
    assert_eq!(polls(&parked), before + 1, "the deadline polls");
    assert!(!standby.timer(attempt), "a timer fires once");
}

#[test]
fn a_timer_set_before_the_overlay_showed_again_is_stale() {
    let (mut standby, handle, _) = overlay();
    handle.update(|_, tx| tx.wake_after(Duration::from_millis(20)));
    let (attempt, _) = turn(&mut standby, true, None).1.expect("a timer");
    standby.shown();
    assert!(!standby.timer(attempt));
}

#[test]
fn a_clipboard_read_asked_while_hidden_is_answered_while_hidden() {
    let (mut standby, handle, parked) = overlay();
    let clipboard = crate::clipboard::memory();
    clipboard.set_text("from another app");
    standby.use_memory(&clipboard);
    handle.update(|_, tx| tx.read_clipboard(|s: &mut State, content, _| s.read = Some(content.text().to_owned())));
    turn(&mut standby, true, None);
    turn(&mut standby, true, None);
    let read = parked.borrow().as_ref().and_then(|p| p.rt.state.read.clone());
    assert_eq!(read.as_deref(), Some("from another app"));
}

#[test]
fn a_callbacks_clipboard_write_lands_and_its_window_requests_settle_while_the_overlay_stays_hidden() {
    let (mut standby, handle, parked) = overlay();
    let clipboard = crate::clipboard::memory();
    standby.use_memory(&clipboard);
    handle.after_hidden(|s, tx| {
        s.runs.push("copied");
        tx.copy("picked entry");
        tx.close_window();
    });
    turn(&mut standby, true, None);
    assert!(turn(&mut standby, true, None).0);
    assert_eq!(clipboard.text().as_deref().map(String::as_str), Some("picked entry"));
    let held = parked.borrow();
    let rt = &held.as_ref().expect("the overlay never mounted").rt;
    assert!(rt.copy_request.is_none());
    assert!(rt.shell_request.is_none(), "a hidden overlay drops the window request it cannot serve");
    assert_eq!(rt.state.runs, ["copied"]);
}

#[test]
fn a_mounted_overlay_steps_on_the_runtime_its_view_holds() {
    let (mut standby, handle, parked) = overlay();
    let Parked { rt: mut mounted, .. } = parked.borrow_mut().take().expect("the runtime before the view mounts");
    handle.after_hidden(|s, _| s.runs.push("mounted"));
    handle.update(|s, _| s.runs.push("update"));
    turn(&mut standby, true, Some(parts(&mut mounted)));
    turn(&mut standby, true, Some(parts(&mut mounted)));
    assert_eq!(mounted.state.runs, ["update", "mounted"]);
}

fn parts(rt: &mut Runtime<State>) -> Mounted<'_> { Mounted { rt, clipboard: None, serves: Serves::default() } }

struct DropFlag(Arc<AtomicBool>);

impl Drop for DropFlag {
    fn drop(&mut self) { self.0.store(true, Ordering::Relaxed); }
}

#[test]
fn closing_the_overlay_drops_its_callbacks_unrun_and_refuses_later_ones() {
    let (mut standby, handle, parked) = overlay();
    let (pending, late) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
    let flag = DropFlag(pending.clone());
    handle.after_hidden(move |s, _| { let _flag = &flag; s.runs.push("pending"); });
    turn(&mut standby, true, None);
    drop(standby);
    assert!(pending.load(Ordering::Relaxed), "the queued callback is dropped with the overlay");
    let flag = DropFlag(late.clone());
    handle.after_hidden(move |s, _| { let _flag = &flag; s.runs.push("late"); });
    assert!(late.load(Ordering::Relaxed), "a callback asked after the close is dropped at once");
    assert!(runs(&parked).is_empty());
}

#[test]
fn an_overlay_that_cannot_mount_drops_its_waiting_callbacks_and_refuses_later_ones() {
    let (mut standby, handle, parked) = overlay();
    standby.shown();
    let (pending, late) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
    let flag = DropFlag(pending.clone());
    handle.after_hidden(move |s, _| { let _flag = &flag; s.runs.push("pending"); });
    turn(&mut standby, false, None);
    parked.borrow_mut().take();
    drop(handle.shared.after_hidden.lock().unwrap().close());
    assert!(!pending.load(Ordering::Relaxed), "the callback waits in the overlay's queue");
    turn(&mut standby, false, None);
    assert!(pending.load(Ordering::Relaxed), "the next turn drops what the closed overlay held");
    let flag = DropFlag(late.clone());
    handle.after_hidden(move |s, _| { let _flag = &flag; s.runs.push("late"); });
    assert!(late.load(Ordering::Relaxed));
}
