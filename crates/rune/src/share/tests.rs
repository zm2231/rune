use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{Cx, El, Env, Runtime, Share, ShareError, ShareResult, Tx, el};

#[derive(Default)]
struct App {
    share: Share,
    heard: Vec<ShareResult>,
}

fn view(_: &App, _: &mut Cx<App>) -> El {
    el().key("share").size(80.0, 20.0).on_press_tx(|s: &mut App, tx: &mut Tx| {
        tx.share(s.share.clone(), "share", |s: &mut App, result, _| s.heard.push(result));
    })
}

fn app(share: Share) -> Runtime<App> {
    let mut rt = Runtime::new(App { share, ..Default::default() }, view, (200.0, 100.0), Env::new(true, 2.0));
    rt.frame(0.0);
    rt
}

fn press(rt: &mut Runtime<App>) {
    rt.pointer_down(10.0, 10.0);
    rt.pointer_up(10.0, 10.0);
    rt.frame(1.0 / 60.0);
}

fn link() -> Share { Share::new().title("Notes").url("https://example.com/notes/42") }

#[test]
fn a_share_waits_open_until_answered_and_its_callback_runs_on_a_later_frame() {
    let mut rt = app(link());
    press(&mut rt);
    assert_eq!(rt.share_json().unwrap()["url"], "https://example.com/notes/42");
    rt.frame(2.0 / 60.0);
    assert!(rt.state.heard.is_empty(), "it stays open across frames");

    rt.resolve_share(Ok(Some("com.apple.UIKit.activity.Mail".into()))).unwrap();
    assert!(rt.share_json().is_none());
    rt.frame(3.0 / 60.0);
    assert_eq!(rt.state.heard, [Ok(Some("com.apple.UIKit.activity.Mail".into()))]);
    assert!(rt.resolve_share(Ok(None)).is_err(), "nothing is open");
}

#[test]
fn a_second_share_while_one_is_open_is_busy_and_an_empty_one_is_refused() {
    let mut rt = app(link());
    press(&mut rt);
    press(&mut rt);
    rt.frame(2.0 / 60.0);
    assert_eq!(rt.state.heard, [Err(ShareError::Busy)]);
    rt.resolve_share(Ok(None)).unwrap();
    rt.frame(3.0 / 60.0);
    assert_eq!(rt.state.heard[1], Ok(None), "the first is still answered");

    let mut rt = app(Share::new().text(""));
    press(&mut rt);
    rt.frame(2.0 / 60.0);
    assert_eq!(rt.state.heard, [Err(ShareError::Empty)]);
    assert!(rt.share_json().is_none());
}

#[test]
fn a_host_shows_it_by_its_anchor_in_window_points_and_answers_later() {
    let mut rt = app(link().file("/tmp/notes.pdf"));
    press(&mut rt);
    let woken = Arc::new(AtomicUsize::new(0));
    let counter = woken.clone();
    let shown = rt.core_mut().take_share(Arc::new(move || { counter.fetch_add(1, Ordering::Relaxed); })).expect("the host takes it");
    assert_eq!(shown.share.files, [std::path::PathBuf::from("/tmp/notes.pdf")]);
    let [x, y, w, h] = shown.anchor.expect("the keyed element anchors it");
    assert!(x >= 0.0 && y >= 0.0 && w > 0.0 && h > 0.0);
    assert!(rt.core_mut().take_share(Arc::new(|| {})).is_none(), "showing, so not taken again");

    rt.frame(2.0 / 60.0);
    assert!(rt.state.heard.is_empty());
    shown.reply.send(Ok(None));
    assert_eq!(woken.load(Ordering::Relaxed), 1);
    rt.frame(3.0 / 60.0);
    rt.frame(4.0 / 60.0);
    assert_eq!(rt.state.heard, [Ok(None)]);
}

#[test]
fn a_host_without_a_share_sheet_refuses_it() {
    let mut rt = app(link());
    press(&mut rt);
    rt.core_mut().refuse_share("no share sheet here");
    rt.frame(2.0 / 60.0);
    assert_eq!(rt.state.heard, [Err(ShareError::Unsupported("no share sheet here".into()))]);
}

#[test]
fn a_window_going_away_answers_its_waiting_or_shown_share_once_while_its_state_lives() {
    let mut rt = app(link());
    press(&mut rt);
    rt.cancel_shares();
    assert_eq!(rt.state.heard, [Ok(None)], "a share the host never took answers at once");
    assert!(rt.share_json().is_none());

    press(&mut rt);
    let shown = rt.core_mut().take_share(Arc::new(|| {})).expect("the host takes it");
    rt.cancel_shares();
    assert_eq!(rt.state.heard, [Ok(None), Ok(None)], "a shown share answers too");
    shown.reply.send(Ok(Some("com.apple.share.Mail".into())));
    rt.frame(2.0 / 60.0);
    rt.frame(3.0 / 60.0);
    assert_eq!(rt.state.heard.len(), 2, "the picker's late answer is ignored");

    rt.update(|_, tx| {
        tx.share(link(), "share", |s: &mut App, result, tx| {
            s.heard.push(result);
            tx.share(link(), "share", |s: &mut App, follow, _| s.heard.push(follow));
        });
    });
    rt.cancel_shares();
    assert_eq!(rt.state.heard[2..], [Ok(None), Ok(None)], "a share its callback asks for is answered as well");
    assert!(rt.share_json().is_none());
}
