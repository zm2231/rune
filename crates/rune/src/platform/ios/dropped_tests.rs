use std::sync::Arc;

use super::*;
use crate::{AppHandle, AppSpec, WindowSpec};

/// The kinds a handle's requests hold after `ask`.
fn kinds(ask: impl FnOnce(&AppHandle<()>)) -> Vec<Kind> {
    let handle = AppHandle::<()> { shared: Arc::default() };
    ask(&handle);
    asked(&handle.shared.take().window_requests).into_iter().filter(|(_, asked)| *asked).map(|(kind, _)| kind).collect()
}

#[test]
fn the_first_request_of_a_kind_is_reported_and_later_ones_are_not() {
    let mut reported = Reported::default();
    assert!(reported.first(Kind::Quit));
    assert!(!reported.first(Kind::Quit));
    assert!(!reported.first(Kind::Quit));
}

#[test]
fn each_kind_is_reported_on_its_own() {
    let mut reported = Reported::default();
    for kind in Kind::ALL { assert!(reported.first(kind), "{kind:?}"); }
    for kind in Kind::ALL { assert!(!reported.first(kind), "{kind:?}"); }
    // One view's reports are not another's.
    assert!(Reported::default().first(Kind::Window));
}

#[test]
fn every_kind_says_something_of_its_own() {
    for (at, kind) in Kind::ALL.into_iter().enumerate() {
        assert!(kind.message().contains(" ignored: "), "{kind:?}");
        for other in &Kind::ALL[at + 1..] { assert_ne!(kind.message(), other.message()); }
    }
}

#[test]
fn updates_and_wakes_are_no_dropped_request() {
    assert_eq!(kinds(|_| {}), []);
    assert_eq!(kinds(|handle| { handle.wake(); handle.update(|_, _| {}); }), []);
}

#[test]
fn each_window_request_is_its_kind() {
    assert_eq!(kinds(|handle| handle.open()), [Kind::Window]);
    assert_eq!(kinds(|handle| handle.show()), [Kind::Window]);
    assert_eq!(kinds(|handle| handle.hide()), [Kind::Window]);
    assert_eq!(kinds(|handle| handle.close()), [Kind::Window]);
    assert_eq!(kinds(|handle| handle.activate("token")), [Kind::Window]);
    assert_eq!(kinds(|handle| handle.focus()), [Kind::Activation]);
    assert_eq!(kinds(|handle| handle.minimize()), [Kind::Activation]);
    assert_eq!(kinds(|handle| handle.set_position((10, 20))), [Kind::Position]);
    assert_eq!(kinds(|handle| handle.set_dock_icon(false)), [Kind::Dock]);
    assert_eq!(kinds(|handle| handle.quit()), [Kind::Quit]);
    assert_eq!(kinds(|handle| handle.relaunch()), [Kind::Quit]);
}

/// The kinds a frame's handlers left.
fn handler_kinds(shell: Option<Shell>, minimize: bool, opened: usize) -> Vec<Kind> {
    asked_by_handlers(shell, minimize, opened).into_iter().filter(|(_, asked)| *asked).map(|(kind, _)| kind).collect()
}

#[test]
fn a_frame_whose_handlers_asked_nothing_drops_nothing() {
    assert_eq!(handler_kinds(None, false, 0), []);
}

#[test]
fn each_handler_request_is_its_kind() {
    assert_eq!(handler_kinds(Some(Shell::Close), false, 0), [Kind::Window]);
    assert_eq!(handler_kinds(Some(Shell::Quit), false, 0), [Kind::Quit]);
    assert_eq!(handler_kinds(Some(Shell::Relaunch), false, 0), [Kind::Quit]);
    assert_eq!(handler_kinds(None, true, 0), [Kind::Activation]);
    assert_eq!(handler_kinds(None, false, 2), [Kind::Opened]);
    assert_eq!(handler_kinds(Some(Shell::Quit), true, 1), [Kind::Opened, Kind::Activation, Kind::Quit]);
}

#[test]
fn a_handlers_requests_latch_in_the_runtime_until_they_are_taken() {
    let mut rt = crate::Runtime::new((), |_, _| crate::el(), (100.0, 100.0), crate::Env::new(true, 1.0));
    rt.frame(0.0);
    rt.update(|_, tx| { tx.quit(); tx.minimize(); });
    let (shell, minimize) = (rt.shell_request.take(), rt.host.minimize.field());
    rt.minimize_step(crate::window::MinimizeEvent::Lost);
    assert_eq!(handler_kinds(shell, minimize, 0), [Kind::Activation, Kind::Quit]);
    // Taken, nothing is left for the next frame to report or to hold.
    assert_eq!(handler_kinds(rt.shell_request.take(), rt.host.minimize.field(), 0), []);
}

#[test]
fn opened_windows_are_taken_so_none_stays_queued() {
    assert!(!crate::is_headless(), "a headless run queues no window");
    let window = || WindowSpec::new("other", AppSpec::new("other", (100.0, 100.0), || (), |_, _| crate::el()));
    assert_eq!(take_opened(), 0);
    let _ = crate::open_window(window());
    let _ = crate::open_window(window());
    assert_eq!(take_opened(), 2);
    assert_eq!(take_opened(), 0, "the queue is empty again");
    for _ in 0..100 {
        let _ = crate::open_window(window());
        assert_eq!(take_opened(), 1);
    }
    assert_eq!(take_opened(), 0);
}

#[test]
fn several_requests_in_one_turn_are_each_found() {
    assert_eq!(kinds(|handle| { handle.show(); handle.focus(); handle.quit(); }), [Kind::Window, Kind::Activation, Kind::Quit]);
}
