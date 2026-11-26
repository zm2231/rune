use super::*;
use crate::{Cx, Env, Runtime, ScrollSpec, col, el};

struct Pane {
    slot: NativeView,
    open: bool,
    height: f32,
    opacity: f32,
}

impl Pane {
    fn new() -> Self { Pane { slot: NativeView::new(), open: true, height: 120.0, opacity: 1.0 } }
}

fn scrolling(p: &Pane, _: &mut Cx<Pane>) -> El {
    col().size(200.0, 100.0).child(
        col().key("list").scroll(ScrollSpec::default()).w_full().h(100.0)
            .child(el().w_full().h(60.0))
            .children(p.open.then(|| el().key("web").w_full().h(p.height).radius(10.0).opacity(p.opacity).native_view(&p.slot)))
            .child(el().w_full().h(400.0)),
    )
}

fn runtime(view: fn(&Pane, &mut Cx<Pane>) -> El) -> Runtime<Pane> {
    let mut rt = Runtime::new(Pane::new(), view, (200.0, 100.0), Env { scale: 2.0, system_dark: false });
    rt.frame(0.0);
    rt
}

fn settle(rt: &mut Runtime<Pane>) {
    for k in 1..=120 { rt.frame(f64::from(k) / 60.0); }
}

#[test]
fn the_view_takes_the_elements_rect_cut_to_the_scroll_viewport() {
    let rt = runtime(scrolling);
    let [d] = rt.native_views() else { panic!("one host: {:?}", rt.native_views()) };
    assert_eq!(d.view, rt.state.slot.id());
    assert_eq!(d.rect, [0.0, 120.0, 400.0, 240.0]);
    assert_eq!(d.clip, [0.0, 120.0, 400.0, 80.0], "the part under the viewport's bottom edge is cut");
    assert_eq!(d.radius, 20.0);
    assert_eq!(d.points(2.0), ([0.0, 60.0, 200.0, 120.0], [0.0, 60.0, 200.0, 40.0], 10.0));
}

#[test]
fn a_scroll_moves_the_view_in_the_frame_that_draws_it() {
    let mut rt = runtime(scrolling);
    rt.wheel(100.0, 50.0, -90.0);
    let mut moved = false;
    for k in 1..=90 {
        rt.frame(f64::from(k) / 120.0);
        let (drawn, _) = rt.presented("web").expect("drawn");
        let d = rt.native_views()[0];
        assert_eq!(d.rect, drawn.map(|v| v * 2.0), "frame {k}: the view sits where this frame drew its element");
        let top = d.rect[1].max(0.0);
        assert_eq!(d.clip[1], top, "frame {k}: the top edge is cut at the viewport");
        moved |= d.rect[1] < 120.0;
    }
    assert!(moved, "the wheel scrolled the content");
}

#[test]
fn scrolled_out_collapsed_or_faded_out_views_are_not_listed() {
    let mut rt = runtime(scrolling);
    rt.wheel(100.0, 50.0, -450.0);
    settle(&mut rt);
    assert!(rt.native_views().is_empty(), "scrolled past: {:?}", rt.native_views());

    let mut rt = runtime(scrolling);
    rt.state.height = 0.0;
    rt.invalidate();
    settle(&mut rt);
    assert!(rt.native_views().is_empty(), "zero height");

    let mut rt = runtime(scrolling);
    rt.state.opacity = 0.0;
    rt.invalidate();
    settle(&mut rt);
    assert!(rt.native_views().is_empty(), "faded out");

    let mut rt = runtime(scrolling);
    rt.state.open = false;
    rt.invalidate();
    settle(&mut rt);
    assert!(rt.native_views().is_empty(), "unmounted");
    rt.state.open = true;
    rt.invalidate();
    settle(&mut rt);
    assert_eq!(rt.native_views().len(), 1, "shown again once it mounts");
}

#[test]
fn opacity_reaches_the_view_with_its_ancestors() {
    fn view(p: &Pane, _: &mut Cx<Pane>) -> El {
        col().size(200.0, 100.0).opacity(0.5).child(el().size(100.0, 50.0).opacity(p.opacity).native_view(&p.slot))
    }
    let mut rt = runtime(view);
    rt.state.opacity = 0.5;
    rt.invalidate();
    settle(&mut rt);
    assert!((rt.native_views()[0].opacity - 0.25).abs() < 1e-4, "{:?}", rt.native_views()[0]);
}

#[test]
fn a_window_resize_lays_the_view_out_again() {
    fn view(p: &Pane, _: &mut Cx<Pane>) -> El {
        col().w_full().h_full().padding(10.0).child(el().w_full().h_full().native_view(&p.slot))
    }
    let mut rt = runtime(view);
    assert_eq!(rt.native_views()[0].points(2.0).0, [10.0, 10.0, 180.0, 80.0]);
    rt.set_viewport((300.0, 160.0), 2.0);
    settle(&mut rt);
    assert_eq!(rt.native_views()[0].points(2.0).0, [10.0, 10.0, 280.0, 140.0]);
}

#[test]
fn views_are_listed_in_paint_order() {
    struct Two { a: NativeView, b: NativeView }
    fn view(t: &Two, _: &mut Cx<Two>) -> El {
        el().size(200.0, 100.0)
            .child(el().absolute(0.0, 0.0).size(100.0, 100.0).native_view(&t.b))
            .child(el().absolute(50.0, 0.0).size(100.0, 100.0).native_view(&t.a))
    }
    let two = Two { a: NativeView::new(), b: NativeView::new() };
    let (a, b) = (two.a.id(), two.b.id());
    let mut rt = Runtime::new(two, view, (200.0, 100.0), Env::default());
    rt.frame(0.0);
    assert_eq!(rt.native_views().iter().map(|d| d.view).collect::<Vec<_>>(), [b, a]);
}

#[test]
fn a_dropped_slot_is_gone_and_its_element_lists_nothing_extra() {
    let slot = NativeView::new();
    let id = slot.id();
    let clone = slot.clone();
    assert!(super::slot(id).is_some());
    drop(slot);
    assert!(super::slot(id).is_some(), "a clone keeps it");
    drop(clone);
    assert!(super::slot(id).is_none());
}

#[test]
fn off_apple_platforms_the_slot_has_no_window_handle() {
    let slot = NativeView::new();
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    assert!(slot.window_handle().is_err());
    assert!(!slot.has_focus());
}

#[test]
fn a_press_inside_the_hosted_view_clears_runes_focus() {
    fn view(p: &Pane, _: &mut Cx<Pane>) -> El {
        col().size(200.0, 100.0)
            .child(el().key("save").size(80.0, 30.0).focusable(true))
            .child(el().size(200.0, 60.0).native_view(&p.slot))
    }
    let mut rt = runtime(view);
    let (x, y) = rt.find("save").unwrap();
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.1);
    assert_eq!(rt.presented_focus_key(), Some("save"));
    let id = rt.state.slot.id();
    press_where(|slot| slot.id == id);
    rt.frame(0.2);
    rt.frame(0.3);
    assert_eq!(rt.presented_focus_key(), None);
}

fn reading_order(p: &Pane, _: &mut Cx<Pane>) -> El {
    col().size(200.0, 300.0)
        .child(crate::text("Before").key("before"))
        .child(el().key("web").size(200.0, 100.0).native_view(&p.slot))
        .child(crate::text("After").key("after"))
}

#[test]
fn the_hosting_element_sits_between_its_siblings_in_the_accessibility_tree() {
    let rt = runtime(reading_order);
    let tree = rt.accessibility_tree();
    let node = |key: &str| tree.nodes.iter().find(|(_, n)| n.author_id() == Some(key)).unwrap_or_else(|| panic!("{key} in the tree"));
    let (web, hosted) = node("web");
    assert_eq!(from_a11y_class(hosted.class_name().unwrap()), Some(rt.state.slot.id()));
    let parent = tree.nodes.iter().find(|(_, n)| n.children().contains(web)).map(|(_, n)| n).expect("a parent");
    let order: Vec<_> = parent.children().iter().map(|c| tree.nodes.iter().find(|(id, _)| id == c).unwrap().1.author_id()).filter(Option::is_some).collect();
    assert_eq!(order, [Some("before"), Some("web"), Some("after")]);
}

#[test]
fn the_ios_reading_list_holds_the_hosted_view_in_its_place() {
    let rt = runtime(reading_order);
    let mut mirror = crate::a11y::ios::Mirror::default();
    mirror.apply(rt.accessibility_tree());
    let list: Vec<_> = mirror.elements().iter().map(|e| (e.label.as_str(), e.hosted)).collect();
    assert_eq!(list, [("Before", None), ("", Some(rt.state.slot.id())), ("After", None)]);
}
