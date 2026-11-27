use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use crate::overlay::OverlayHandle;
use crate::runtime::Env;
use crate::{Cx, El, el};

#[derive(Default)]
struct State {
    runs: Vec<&'static str>,
}

fn view(_: &State, _: &mut Cx<State>) -> El { el() }

fn runtime() -> Runtime<State> { Runtime::new(State::default(), view, (40.0, 20.0), Env::new(false, 1.0)) }

fn overlay() -> (AfterHidden, OverlayHandle<State>, Unmounted<State>) {
    let shared = Arc::new(Shared::default());
    let unmounted = Rc::new(RefCell::new(Some(runtime())));
    (AfterHidden::new(shared.clone(), unmounted.clone()), OverlayHandle { shared }, unmounted)
}

fn runs(unmounted: &Unmounted<State>) -> Vec<&'static str> {
    unmounted.borrow().as_ref().map(|rt| rt.state.runs.clone()).unwrap_or_default()
}

fn turn(after_hidden: &mut AfterHidden, hidden: bool, mounted: Option<&mut dyn Any>) -> Turn {
    let mut all = after_hidden.new_turn();
    let queued = after_hidden.queued(hidden);
    let end = if hidden { after_hidden.end_of_turn() } else { Turn::default() };
    all.wake |= queued.wake || end.wake;
    all.run |= queued.run || end.run;
    if all.run { after_hidden.run(mounted); }
    all
}

#[test]
fn a_callback_asked_while_shown_runs_on_the_turn_after_the_hide_and_not_before() {
    let (mut after_hidden, handle, unmounted) = overlay();
    after_hidden.shown();
    handle.after_hidden(|s, _| s.runs.push("picked"));
    assert_eq!(turn(&mut after_hidden, false, None), Turn::default());
    let hide = after_hidden.hidden();
    assert_eq!(hide, Turn { wake: true, run: false }, "the hide asks for another turn");
    assert!(!after_hidden.end_of_turn().run, "nothing runs in the turn the overlay hid");
    assert!(runs(&unmounted).is_empty());
    assert!(turn(&mut after_hidden, true, None).run);
    assert_eq!(runs(&unmounted), ["picked"]);
    assert_eq!(turn(&mut after_hidden, true, None), Turn::default(), "a callback runs once");
}

#[test]
fn a_callback_asked_while_hidden_runs_on_the_next_turn() {
    let (mut after_hidden, handle, unmounted) = overlay();
    handle.after_hidden(|s, _| s.runs.push("late"));
    let first = turn(&mut after_hidden, true, None);
    assert_eq!(first, Turn { wake: true, run: false });
    assert!(turn(&mut after_hidden, true, None).run);
    assert_eq!(runs(&unmounted), ["late"]);
}

#[test]
fn showing_again_before_the_next_turn_keeps_the_callback_for_the_next_hide() {
    let (mut after_hidden, handle, unmounted) = overlay();
    after_hidden.shown();
    handle.after_hidden(|s, _| s.runs.push("picked"));
    turn(&mut after_hidden, false, None);
    after_hidden.hidden();
    after_hidden.shown();
    assert!(!turn(&mut after_hidden, false, None).run);
    after_hidden.hidden();
    assert!(turn(&mut after_hidden, true, None).run);
    assert_eq!(runs(&unmounted), ["picked"]);
}

#[test]
fn a_mounted_overlay_runs_its_callbacks_on_the_runtime_its_view_holds() {
    let (mut after_hidden, handle, unmounted) = overlay();
    let mut mounted = unmounted.borrow_mut().take().expect("the runtime before the view mounts");
    handle.after_hidden(|s, _| s.runs.push("mounted"));
    turn(&mut after_hidden, true, Some(&mut mounted));
    turn(&mut after_hidden, true, Some(&mut mounted));
    assert_eq!(mounted.state.runs, ["mounted"]);
}

struct DropFlag(Arc<AtomicBool>);

impl Drop for DropFlag {
    fn drop(&mut self) { self.0.store(true, Ordering::Relaxed); }
}

#[test]
fn closing_the_overlay_drops_its_callbacks_unrun_and_refuses_later_ones() {
    let (mut after_hidden, handle, unmounted) = overlay();
    let (pending, late) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
    let flag = DropFlag(pending.clone());
    handle.after_hidden(move |s, _| { let _flag = &flag; s.runs.push("pending"); });
    turn(&mut after_hidden, true, None);
    drop(after_hidden);
    assert!(pending.load(Ordering::Relaxed), "the queued callback is dropped with the overlay");
    let flag = DropFlag(late.clone());
    handle.after_hidden(move |s, _| { let _flag = &flag; s.runs.push("late"); });
    assert!(late.load(Ordering::Relaxed), "a callback asked after the close is dropped at once");
    assert!(runs(&unmounted).is_empty());
}

#[test]
fn an_overlay_that_cannot_mount_drops_its_waiting_callbacks_and_refuses_later_ones() {
    let (mut after_hidden, handle, unmounted) = overlay();
    after_hidden.shown();
    let (pending, late) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
    let flag = DropFlag(pending.clone());
    handle.after_hidden(move |s, _| { let _flag = &flag; s.runs.push("pending"); });
    turn(&mut after_hidden, false, None);
    unmounted.borrow_mut().take();
    drop(handle.shared.after_hidden.lock().unwrap().close());
    assert!(!pending.load(Ordering::Relaxed), "the callback waits in the overlay's queue");
    turn(&mut after_hidden, false, None);
    assert!(pending.load(Ordering::Relaxed), "the next turn drops what the closed overlay held");
    let flag = DropFlag(late.clone());
    handle.after_hidden(move |s, _| { let _flag = &flag; s.runs.push("late"); });
    assert!(late.load(Ordering::Relaxed));
}
