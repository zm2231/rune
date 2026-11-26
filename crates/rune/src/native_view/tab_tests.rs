use super::headless::Call;
use super::*;
use crate::{Cx, Env, Key, KeyEvent, Mods, Runtime, col, el};

struct Page {
    web: NativeView,
    map: NativeView,
    second: bool,
    twin: bool,
    modal: bool,
}

impl Page {
    fn new() -> Self {
        let page = Page { web: NativeView::new(), map: NativeView::new(), second: false, twin: false, modal: false };
        page.web.slot.view.built.set(true);
        page.map.slot.view.built.set(true);
        page
    }
}

fn button(key: &'static str) -> El { el().key(key).size(80.0, 30.0).focusable(true) }

fn view(p: &Page, _: &mut Cx<Page>) -> El {
    col().size(300.0, 400.0)
        .child(button("first"))
        .child(button("before"))
        .child(el().key("web").size(200.0, 100.0).native_view(&p.web))
        .children(p.second.then(|| el().key("map").size(200.0, 100.0).native_view(&p.map)))
        .children(p.twin.then(|| el().key("twin").size(200.0, 100.0).native_view(&p.web.clone())))
        .child(button("after"))
        .children(p.modal.then(|| col().key("sheet").absolute(0.0, 300.0).size(300.0, 100.0).modal(false, |p: &mut Page| p.modal = false).child(button("ok"))))
}

fn runtime(page: Page) -> Runtime<Page> {
    let mut rt = Runtime::new(page, view, (300.0, 400.0), Env::default());
    rt.frame(0.0);
    rt
}

fn tab(rt: &mut Runtime<Page>, back: bool) {
    rt.key(KeyEvent { key: Key::Tab, mods: Mods { shift: back, ..Mods::default() } });
    rt.frame(0.1);
}

fn frames(rt: &mut Runtime<Page>) {
    rt.frame(0.2);
    rt.frame(0.3);
}

fn calls(slot: &NativeView) -> Vec<Call> { slot.slot.view.calls.take() }

fn focus(rt: &mut Runtime<Page>, key: &str) {
    let (x, y) = rt.find(key).expect("the control is drawn");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.05);
    assert_eq!(rt.presented_focus_key(), Some(key));
}

#[test]
fn tab_from_the_control_before_a_host_enters_it_from_its_start() {
    let mut rt = runtime(Page::new());
    focus(&mut rt, "before");
    tab(&mut rt, false);
    assert_eq!(calls(&rt.state.web), [Call::Enter { back: false }]);
    assert_eq!(rt.presented_focus_key(), None, "Rune's focus clears while the view has the keyboard");
}

#[test]
fn shift_tab_from_the_control_after_a_host_enters_it_from_its_end() {
    let mut rt = runtime(Page::new());
    focus(&mut rt, "after");
    tab(&mut rt, true);
    assert_eq!(calls(&rt.state.web), [Call::Enter { back: true }]);
    assert_eq!(rt.presented_focus_key(), None);
}

#[test]
fn tabbing_out_of_a_host_focuses_its_neighbour_and_takes_the_keyboard_back() {
    let mut rt = runtime(Page::new());
    focus(&mut rt, "before");
    tab(&mut rt, false);
    calls(&rt.state.web);
    rt.state.web.tabbed_out(false);
    frames(&mut rt);
    assert_eq!(rt.presented_focus_key(), Some("after"));
    assert!(rt.kbd.focus.keyboard(), "the ring shows, as for any Tab");
    assert_eq!(calls(&rt.state.web), [Call::Reclaim]);

    tab(&mut rt, true);
    assert_eq!(calls(&rt.state.web), [Call::Enter { back: true }]);
    rt.state.web.tabbed_out(true);
    frames(&mut rt);
    assert_eq!(rt.presented_focus_key(), Some("before"));
    assert_eq!(calls(&rt.state.web), [Call::Reclaim]);
}

#[test]
fn a_press_inside_a_host_then_tab_out_goes_on_from_the_host() {
    let mut rt = runtime(Page::new());
    focus(&mut rt, "first");
    let id = rt.state.web.id();
    press_where(|slot| slot.id == id);
    frames(&mut rt);
    assert_eq!(rt.presented_focus_key(), None);
    rt.state.web.tabbed_out(false);
    frames(&mut rt);
    assert_eq!(rt.presented_focus_key(), Some("after"));
    assert_eq!(calls(&rt.state.web), [Call::Reclaim]);
}

#[test]
fn tab_wraps_through_the_host_at_either_end() {
    let mut rt = runtime(Page::new());
    focus(&mut rt, "after");
    tab(&mut rt, false);
    assert_eq!(rt.presented_focus_key(), Some("first"));
    tab(&mut rt, true);
    assert_eq!(rt.presented_focus_key(), Some("after"));
    assert!(calls(&rt.state.web).is_empty(), "the host sits between before and after, not at the ends");
}

#[test]
fn tab_out_of_one_host_enters_the_next_directly() {
    let mut rt = runtime(Page { second: true, ..Page::new() });
    rt.state.web.tabbed_out(false);
    frames(&mut rt);
    assert_eq!(calls(&rt.state.map), [Call::Enter { back: false }]);
    assert!(calls(&rt.state.web).is_empty(), "the keyboard goes host to host without Rune's view");
    rt.state.map.tabbed_out(true);
    frames(&mut rt);
    assert_eq!(calls(&rt.state.web), [Call::Enter { back: true }]);
    assert_eq!(rt.presented_focus_key(), None);
}

#[test]
fn an_empty_slot_is_not_a_tab_stop() {
    let page = Page::new();
    page.web.slot.view.built.set(false);
    let mut rt = runtime(page);
    focus(&mut rt, "before");
    tab(&mut rt, false);
    assert_eq!(rt.presented_focus_key(), Some("after"));
    assert!(calls(&rt.state.web).is_empty());
}

#[test]
fn a_host_behind_a_modal_is_not_a_tab_stop() {
    let mut rt = runtime(Page { modal: true, ..Page::new() });
    focus(&mut rt, "ok");
    tab(&mut rt, false);
    tab(&mut rt, true);
    assert_eq!(rt.presented_focus_key(), Some("ok"));
    assert!(calls(&rt.state.web).is_empty());
}

#[test]
fn a_focus_request_while_a_host_has_the_keyboard_takes_it_back() {
    let mut rt = runtime(Page::new());
    focus(&mut rt, "before");
    tab(&mut rt, false);
    calls(&rt.state.web);
    rt.update(|_, tx| tx.focus("first", true));
    rt.frame(0.2);
    assert_eq!(rt.presented_focus_key(), Some("first"));
    assert_eq!(calls(&rt.state.web), [Call::Reclaim]);
}

#[test]
fn a_slot_shown_twice_is_one_tab_stop() {
    let mut rt = runtime(Page { twin: true, ..Page::new() });
    focus(&mut rt, "before");
    tab(&mut rt, false);
    assert_eq!(calls(&rt.state.web), [Call::Enter { back: false }]);
    rt.state.web.tabbed_out(false);
    frames(&mut rt);
    assert_eq!(rt.presented_focus_key(), Some("after"));
    assert_eq!(calls(&rt.state.web), [Call::Reclaim], "Tab leaves the view rather than entering it again");
    tab(&mut rt, true);
    assert_eq!(calls(&rt.state.web), [Call::Enter { back: true }]);
}
