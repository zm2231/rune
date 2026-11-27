use super::tests::{assert_same_state, maybe_memo};
use crate::{Align, Cx, El, Env, Key, KeyEvent, Mods, Runtime, Side, col, el, row};

#[derive(Clone, Copy)]
struct Shift {
    top: f32,
    memo: bool,
    fit: bool,
}

fn view(s: &Shift, _: &mut Cx<Shift>) -> El {
    let fit = s.fit;
    let item = maybe_memo(s.memo, "row", 0, move || {
        let badge = el().key("badge").size(6.0, 6.0);
        let badge = if fit { badge.anchored_fit("dot", Side::Right, Align::Center, 2.0) } else { badge.anchored("dot", Side::Right, Align::Center, 2.0) };
        row().child(el().key("dot").size(10.0, 10.0)).child(badge)
    });
    col().w_full().child(el().key("spacer").size(10.0, s.top)).child(item)
}

fn badge_epoch(rt: &Runtime<Shift>) -> u32 {
    rt.tree.nodes.iter().find(|n| n.key.as_deref() == Some("badge")).unwrap().epoch
}

/// A memoized row holding an anchored overlay draws as a fresh build does while the row moves,
/// fitted to the window or not; one that isn't fitted keeps its nodes from the last commit.
#[test]
fn a_memoized_anchored_overlay_follows_its_moving_anchor_as_a_fresh_build_does() {
    for fit in [false, true] {
        let mut memo_rt = Runtime::new(Shift { top: 10.0, memo: true, fit }, view, (400.0, 400.0), Env::new(true, 1.0));
        let mut fresh_rt = Runtime::new(Shift { top: 10.0, memo: false, fit }, view, (400.0, 400.0), Env::new(true, 1.0));
        memo_rt.frame(0.0);
        fresh_rt.frame(0.0);
        assert_same_state(&memo_rt, &fresh_rt, &format!("fit {fit}: first frame"));
        let epoch = badge_epoch(&memo_rt);
        for (step, top) in [60.0, 25.0].into_iter().enumerate() {
            for rt in [&mut memo_rt, &mut fresh_rt] {
                rt.update(|s, tx| { s.top = top; tx.instant(); });
                rt.frame(1.0 / 60.0);
            }
            assert_same_state(&memo_rt, &fresh_rt, &format!("fit {fit}: move {step}"));
        }
        if !fit {
            assert_eq!(badge_epoch(&memo_rt), epoch, "the memoized overlay was reused, not built again");
        }
    }
}

fn modal_view(s: &Shift, _: &mut Cx<Shift>) -> El {
    let sheet = maybe_memo(s.memo, "dialog", 0, || {
        col().child(col().key("sheet").modal_trap().child(el().key("ok").size(40.0, 20.0).focusable(true)).child(el().key("cancel").size(40.0, 20.0).focusable(true)))
    });
    col().w_full().child(el().key("outside").size(40.0, 20.0).focusable(true)).child(el().key("spacer").size(10.0, s.top)).child(sheet)
}

/// A memoized dialog keeps its nodes when the view changes around it, and still traps focus as a
/// fresh build does.
#[test]
fn a_memoized_modal_is_reused_and_still_traps_focus() {
    let mut memo_rt = Runtime::new(Shift { top: 10.0, memo: true, fit: false }, modal_view, (400.0, 400.0), Env::new(true, 1.0));
    let mut fresh_rt = Runtime::new(Shift { top: 10.0, memo: false, fit: false }, modal_view, (400.0, 400.0), Env::new(true, 1.0));
    memo_rt.frame(0.0);
    fresh_rt.frame(0.0);
    let epoch = |rt: &Runtime<Shift>| rt.tree.nodes.iter().find(|n| n.key.as_deref() == Some("sheet")).unwrap().epoch;
    let sheet = epoch(&memo_rt);
    for rt in [&mut memo_rt, &mut fresh_rt] {
        rt.update(|s, tx| { s.top = 60.0; tx.instant(); });
        rt.frame(1.0 / 60.0);
    }
    assert_same_state(&memo_rt, &fresh_rt, "moved");
    assert_eq!(epoch(&memo_rt), sheet, "the memoized dialog was reused, not built again");
    let modal = |rt: &Runtime<Shift>| rt.core().active_modal().map(|i| rt.tree.nodes[i].path.clone());
    assert!(modal(&memo_rt).is_some_and(|p| p.ends_with("sheet")), "{:?}", modal(&memo_rt));
    assert_eq!(modal(&memo_rt), modal(&fresh_rt));
    let focused = |rt: &Runtime<Shift>| rt.elements().into_iter().find(|e| e.focused).and_then(|e| e.key);
    for step in 0..3 {
        for rt in [&mut memo_rt, &mut fresh_rt] {
            rt.key(KeyEvent { key: Key::Tab, mods: Mods::default() });
            rt.frame(1.0 / 60.0);
        }
        let now = focused(&memo_rt);
        assert!(matches!(now.as_deref(), Some("ok" | "cancel")), "tab {step}: focus left the dialog for {now:?}");
        assert_eq!(now, focused(&fresh_rt), "tab {step}");
    }
}

#[derive(Clone, Copy)]
struct Outside {
    top: f32,
    anchor: bool,
    memo: bool,
}

fn outside_anchor_view(s: &Outside, _: &mut Cx<Outside>) -> El {
    let item = maybe_memo(s.memo, "row", 0, || row().child(el().key("dot").size(10.0, 10.0)).child(el().key("badge").size(6.0, 6.0).anchored("target", Side::Bottom, Align::Start, 2.0)));
    let page = col().w_full().child(el().key("spacer").size(10.0, s.top)).child(item);
    if s.anchor { page.child(el().key("target").size(30.0, 12.0)) } else { page }
}

/// A memoized overlay anchored to an element outside it, mounted after it, is placed as a fresh
/// build places it while that anchor moves, goes and comes back.
#[test]
fn a_memoized_overlay_follows_an_anchor_outside_it_that_moves_goes_and_returns() {
    let start = |memo| Outside { top: 10.0, anchor: true, memo };
    let mut memo_rt = Runtime::new(start(true), outside_anchor_view, (400.0, 400.0), Env::new(true, 1.0));
    let mut fresh_rt = Runtime::new(start(false), outside_anchor_view, (400.0, 400.0), Env::new(true, 1.0));
    memo_rt.frame(0.0);
    fresh_rt.frame(0.0);
    assert_same_state(&memo_rt, &fresh_rt, "first frame");
    let epoch = |rt: &Runtime<Outside>| rt.tree.nodes.iter().find(|n| n.key.as_deref() == Some("badge")).unwrap().epoch;
    let badge = epoch(&memo_rt);
    for (step, (top, anchor)) in [(50.0, true), (50.0, false), (20.0, false), (20.0, true), (70.0, true)].into_iter().enumerate() {
        for rt in [&mut memo_rt, &mut fresh_rt] {
            rt.update(|s, tx| { s.top = top; s.anchor = anchor; tx.instant(); });
            rt.frame(1.0 / 60.0);
        }
        assert_same_state(&memo_rt, &fresh_rt, &format!("step {step}: top {top}, anchor {anchor}"));
    }
    assert_eq!(epoch(&memo_rt), badge, "the memoized overlay was reused throughout");
}
