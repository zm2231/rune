use super::*;
use crate::el::{col, el, row};
use crate::{Cx, El, Env, ScrollSpec, Transition};

const SIZE: (f32, f32) = (300.0, 600.0);
/// What the status bar takes of the view's top.
const BAR: f32 = 50.0;

#[derive(Clone, Copy, PartialEq)]
enum Scene {
    /// One list filling the view.
    List,
    /// A title bar over the list.
    Titled,
    /// A list of lists: both overflow.
    Nested,
    /// A list in a viewport whose own content fits.
    Wrapped,
    /// A modal with its own list over the list.
    Modal,
    /// A horizontal pager of two vertical pages, scrolled to its second.
    Pager,
    /// A narrow column and a wide one, side by side.
    Columns,
    /// A short list over a text area that overflows.
    Composer,
    /// A sheet with its own list over the list, which fades out when the scene leaves it.
    Sheet,
    /// Nothing that scrolls.
    Still,
    /// A list of ten thousand virtual rows.
    Virtual,
    /// A list that follows its end.
    Chat,
    Zoomed,
}

fn rows(n: usize) -> impl Iterator<Item = El> {
    (0..n).map(|i| el().key(format!("row{i}")).h(40.0))
}

fn list(key: &str, n: usize) -> El {
    col().key(key).scroll(ScrollSpec { stick_bottom: false }).grow(1.0).children(rows(n))
}

/// A list of a height of its own, where `list` takes what its parent has left.
fn fixed(key: &str, n: usize, h: f32) -> El {
    col().key(key).w_full().h(h).scroll(ScrollSpec { stick_bottom: false }).children(rows(n))
}

fn view(scene: &Scene, cx: &mut Cx<Scene>) -> El {
    let root = col().size(SIZE.0, SIZE.1);
    match scene {
        Scene::List => root.child(list("list", 60)),
        Scene::Titled => root.child(el().key("title").h(BAR + 44.0)).child(list("list", 60)),
        Scene::Nested => root.child(list("outer", 0).child(el().w_full().h(300.0)).child(fixed("inner", 60, 900.0))),
        Scene::Wrapped => root.child(list("outer", 0).child(list("inner", 60).h(SIZE.1))),
        Scene::Modal => root.child(list("list", 60))
            .child(col().key("sheet").absolute(0.0, 0.0).size(SIZE.0, SIZE.1).modal(false, |_: &mut Scene| {}).child(list("sheet-list", 60))),
        Scene::Pager => root.child(row().key("pager").scroll_x().grow(1.0)
            .child(col().w(SIZE.0).child(list("first", 60)))
            .child(col().w(SIZE.0).child(list("second", 60)))),
        Scene::Columns => root.child(row().grow(1.0)
            .child(col().w(100.0).child(list("side", 60)))
            .child(col().w(200.0).child(list("detail", 60)))),
        Scene::Composer => root.child(fixed("list", 3, 480.0)).child(fixed("composer", 30, 120.0)),
        Scene::Sheet => root.child(list("list", 60))
            .child(col().key("sheet").absolute(0.0, 0.0).size(SIZE.0, SIZE.1).transition(Transition::opacity()).child(list("sheet-list", 60))),
        Scene::Still => root.child(el().key("page").grow(1.0)),
        Scene::Virtual => root.child(col().key("list").scroll(ScrollSpec { stick_bottom: false }).grow(1.0)
            .child(col().key("rows").w_full().growth(crate::GrowthSpec::default()).virtual_rows(10_000, 40.0, |_: &Scene, i| format!("row{i}").into(), |_: &Scene, i| el().h(40.0 + (i % 3) as f32 * 10.0)))),
        Scene::Chat => root.child(col().key("list").scroll(ScrollSpec { stick_bottom: true }).grow(1.0).children(rows(60))),
        Scene::Zoomed => {
            cx.zoom(2.0);
            col().size(SIZE.0 / 2.0, SIZE.1 / 2.0).child(list("list", 60))
        }
    }
}

fn scene(scene: Scene) -> Runtime<Scene> {
    let mut rt = Runtime::new(scene, view, SIZE, Env::new(false, 1.0));
    rt.set_safe_area([BAR, 0.0, 0.0, 0.0]);
    rt.frame(0.0);
    rt.frame(1.0 / 120.0);
    rt
}

fn state<'a>(rt: &'a Runtime<Scene>, key: &str) -> &'a ScrollState {
    let node = rt.tree.nodes.iter().find(|n| n.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no {key}"));
    &rt.scroll.scrolls[&node.id]
}

fn offset(rt: &Runtime<Scene>, key: &str) -> f64 { state(rt, key).offset }

/// Scrolls the viewport under (x, y) down by `by` points in one wheel gesture, and lets it
/// rest. The gesture ends, so the next one picks its viewport afresh.
fn down(rt: &mut Runtime<Scene>, (x, y): (f32, f32), by: f32) {
    rt.wheel_began();
    rt.wheel(x, y, -by);
    rt.wheel_ended();
    idle(rt);
}

fn idle(rt: &mut Runtime<Scene>) {
    for _ in 0..600 {
        if !rt.frame(1.0 / 120.0) && !rt.animating { return; }
    }
    panic!("still moving after five seconds");
}

fn main_key(rt: &Runtime<Scene>) -> Option<&str> {
    rt.core().main_viewport().and_then(|i| rt.tree.nodes[i].key.as_deref())
}

#[test]
fn the_line_runs_down_the_centre_from_under_the_status_bar() {
    let points: Vec<_> = line((300.0, 100.0), 50.0).collect();
    assert_eq!(points.first(), Some(&(150.0, 51.0)));
    assert_eq!(points.last(), Some(&(150.0, 99.0)));
    assert!(points.windows(2).all(|p| p[1].1 - p[0].1 == STEP));
    assert_eq!(line((300.0, 40.0), 50.0).count(), 0, "a view the status bar covers has no line");
    assert_eq!(line((300.0, 100.0), -3.0).next(), Some((150.0, 1.0)), "no inset is the view's top");
}

#[test]
fn a_tap_glides_the_list_to_its_top() {
    let mut rt = scene(Scene::List);
    down(&mut rt, (150.0, 300.0), 900.0);
    assert_eq!(offset(&rt, "list"), 900.0);
    assert!(rt.scroll_main_to_top(), "a list off its top is sent there");
    assert_eq!(state(&rt, "list").glide_to(), Some(0.0), "on a glide");
    rt.frame(1.0 / 120.0);
    let on_its_way = offset(&rt, "list");
    assert!(on_its_way > 0.0 && on_its_way < 900.0, "partway after a frame: {on_its_way}");
    idle(&mut rt);
    assert_eq!(offset(&rt, "list"), 0.0);
}

#[test]
fn a_long_trip_starts_two_viewports_out() {
    let mut rt = scene(Scene::List);
    down(&mut rt, (150.0, 300.0), 1800.0);
    assert_eq!(offset(&rt, "list"), 1800.0);
    assert!(rt.scroll_main_to_top());
    assert_eq!(offset(&rt, "list"), 2.0 * 600.0, "cut to two viewports from the top, as a glide to the bottom is");
}

#[test]
fn a_list_at_its_top_is_left_alone() {
    let mut rt = scene(Scene::List);
    assert_eq!(main_key(&rt), Some("list"));
    assert!(!rt.scroll_main_to_top());
    assert_eq!(state(&rt, "list").glide_to(), None);
    assert!(!rt.commit.dirty, "and nothing is asked of the next frame");
}

#[test]
fn nothing_scrollable_is_left_alone() {
    let mut rt = scene(Scene::Still);
    assert_eq!(main_key(&rt), None);
    assert!(!rt.scroll_main_to_top());
}

#[test]
fn a_list_under_a_title_bar_is_found_below_it() {
    let mut rt = scene(Scene::Titled);
    down(&mut rt, (150.0, 300.0), 400.0);
    assert_eq!(main_key(&rt), Some("list"), "the first viewport down the centre line");
    assert!(rt.scroll_main_to_top());
    idle(&mut rt);
    assert_eq!(offset(&rt, "list"), 0.0);
}

#[test]
fn of_nested_lists_the_outermost_goes() {
    let mut rt = scene(Scene::Nested);
    // The inner list fills the view once the outer one has scrolled its header away.
    down(&mut rt, (150.0, 100.0), 300.0);
    down(&mut rt, (150.0, 300.0), 500.0);
    assert_eq!((offset(&rt, "outer"), offset(&rt, "inner")), (300.0, 500.0));
    assert_eq!(main_key(&rt), Some("outer"));
    assert!(rt.scroll_main_to_top());
    idle(&mut rt);
    assert_eq!((offset(&rt, "outer"), offset(&rt, "inner")), (0.0, 500.0), "the inner list stays where it was read");
}

#[test]
fn a_viewport_whose_content_fits_passes_the_tap_to_the_list_inside_it() {
    let mut rt = scene(Scene::Wrapped);
    down(&mut rt, (150.0, 300.0), 500.0);
    assert_eq!(state(&rt, "outer").max, 0.0);
    assert_eq!(main_key(&rt), Some("inner"));
    assert!(rt.scroll_main_to_top());
    idle(&mut rt);
    assert_eq!(offset(&rt, "inner"), 0.0);
}

#[test]
fn a_modals_list_goes_and_the_list_behind_it_stays() {
    let mut rt = scene(Scene::List);
    down(&mut rt, (150.0, 300.0), 700.0);
    rt.update(|s, _| *s = Scene::Modal);
    idle(&mut rt);
    down(&mut rt, (150.0, 300.0), 400.0);
    assert_eq!((offset(&rt, "list"), offset(&rt, "sheet-list")), (700.0, 400.0));
    assert_eq!(main_key(&rt), Some("sheet-list"));
    assert!(rt.scroll_main_to_top());
    idle(&mut rt);
    assert_eq!((offset(&rt, "list"), offset(&rt, "sheet-list")), (700.0, 0.0));
    // With the modal's list at its top, the list behind is still out of reach.
    assert!(!rt.scroll_main_to_top());
    assert_eq!(offset(&rt, "list"), 700.0);
}

#[test]
fn a_list_on_its_way_out_is_passed_over_for_the_one_under_it() {
    let mut rt = scene(Scene::List);
    down(&mut rt, (150.0, 300.0), 700.0);
    rt.update(|s, _| *s = Scene::Sheet);
    idle(&mut rt);
    down(&mut rt, (150.0, 300.0), 400.0);
    assert_eq!((offset(&rt, "list"), offset(&rt, "sheet-list")), (700.0, 400.0));
    assert_eq!(main_key(&rt), Some("sheet-list"));
    // The sheet is dismissed and fades: its list is still drawn over the one behind.
    rt.update(|s, _| *s = Scene::List);
    rt.frame(1.0 / 120.0);
    let leaving = rt.tree.nodes.iter().position(|n| n.key.as_deref() == Some("sheet-list")).expect("the sheet's list is still mounted");
    assert!(rt.core().subtree_exiting(leaving));
    assert_eq!(main_key(&rt), Some("list"));
    assert!(rt.scroll_main_to_top());
    idle(&mut rt);
    assert_eq!(offset(&rt, "list"), 0.0);
}

#[test]
fn of_a_pagers_pages_the_one_showing_goes() {
    let mut rt = scene(Scene::Pager);
    down(&mut rt, (150.0, 300.0), 600.0);
    rt.update(|_, tx| tx.scroll_to_x("pager", SIZE.0));
    idle(&mut rt);
    down(&mut rt, (150.0, 300.0), 450.0);
    assert_eq!((offset(&rt, "first"), offset(&rt, "second")), (600.0, 450.0));
    assert_eq!(main_key(&rt), Some("second"));
    assert!(rt.scroll_main_to_top());
    idle(&mut rt);
    assert_eq!((offset(&rt, "first"), offset(&rt, "second")), (600.0, 0.0), "the page out of sight keeps its place");
    assert_eq!(state(&rt, "pager").x.offset, f64::from(SIZE.0), "and the pager stays on its page");
}

#[test]
fn of_columns_side_by_side_the_one_under_the_centre_goes() {
    let mut rt = scene(Scene::Columns);
    down(&mut rt, (50.0, 300.0), 300.0);
    down(&mut rt, (200.0, 300.0), 500.0);
    assert_eq!((offset(&rt, "side"), offset(&rt, "detail")), (300.0, 500.0));
    assert_eq!(main_key(&rt), Some("detail"), "the centre line at x 150 is in the wide column");
    assert!(rt.scroll_main_to_top());
    idle(&mut rt);
    assert_eq!((offset(&rt, "side"), offset(&rt, "detail")), (300.0, 0.0));
}

#[test]
fn a_text_area_under_a_short_list_does_not_jump() {
    let mut rt = scene(Scene::Composer);
    down(&mut rt, (150.0, 550.0), 200.0);
    assert_eq!(state(&rt, "list").max, 0.0, "the list fits");
    assert_eq!(offset(&rt, "composer"), 200.0);
    assert_eq!(main_key(&rt), None, "the list is the first viewport down the line, and has nowhere to go");
    assert!(!rt.scroll_main_to_top());
    assert_eq!(offset(&rt, "composer"), 200.0);
}

#[test]
fn virtual_rows_land_on_the_first_row() {
    let mut rt = scene(Scene::Virtual);
    down(&mut rt, (150.0, 300.0), 200_000.0);
    assert!(offset(&rt, "list") > 100_000.0);
    assert!(rt.tree.nodes.iter().all(|n| n.key.as_deref() != Some("row0")), "the first row is not mounted this far down");
    assert!(rt.scroll_main_to_top());
    idle(&mut rt);
    assert_eq!(offset(&rt, "list"), 0.0);
    let first = rt.tree.nodes.iter().find(|n| n.key.as_deref() == Some("row0")).expect("the first row is mounted");
    assert_eq!(first.target[1], rt.tree.nodes.iter().find(|n| n.key.as_deref() == Some("list")).unwrap().target[1], "at the list's top");
}

#[test]
fn reduced_motion_jumps() {
    fn reduced(scene: &Scene, cx: &mut Cx<Scene>) -> El {
        cx.reduced_motion(true);
        view(scene, cx)
    }
    let mut rt = Runtime::new(Scene::List, reduced, SIZE, Env::new(false, 1.0));
    rt.frame(0.0);
    down(&mut rt, (150.0, 300.0), 900.0);
    assert!(rt.scroll_main_to_top());
    assert_eq!((offset(&rt, "list"), state(&rt, "list").glide_to()), (0.0, None), "there at once, with no glide");
}

#[test]
fn a_list_that_follows_its_end_stops_following_it() {
    let mut rt = scene(Scene::Chat);
    idle(&mut rt);
    let end = state(&rt, "list").max;
    assert!(end > 0.0 && state(&rt, "list").pinned() && offset(&rt, "list") == end, "it opens on its end");
    assert!(rt.scroll_main_to_top());
    assert!(!state(&rt, "list").pinned(), "the glide takes it off its end");
    idle(&mut rt);
    assert_eq!(offset(&rt, "list"), 0.0);
    assert!(!state(&rt, "list").pinned(), "and at the top it follows nothing");
}

#[test]
fn a_hand_takes_the_list_back_mid_glide() {
    let mut rt = scene(Scene::List);
    down(&mut rt, (150.0, 300.0), 900.0);
    assert!(rt.scroll_main_to_top());
    for _ in 0..5 { rt.frame(1.0 / 120.0); }
    rt.wheel(150.0, 300.0, -10.0);
    let held = offset(&rt, "list");
    assert_eq!(state(&rt, "list").glide_to(), None, "the wheel ends the glide");
    idle(&mut rt);
    assert_eq!(offset(&rt, "list"), held);
}


#[test]
fn the_main_scroll_reports_the_lists_offset_and_reach() {
    let mut rt = scene(Scene::Titled);
    let [offset, max] = rt.main_scroll().expect("the list under the title bar");
    assert_eq!((offset, max), (0.0, max_of(&rt, "list")));
    down(&mut rt, (150.0, 300.0), 500.0);
    assert_eq!(rt.main_scroll(), Some([500.0, max_of(&rt, "list")]));
    assert_eq!(scene(Scene::Still).main_scroll(), None, "nothing scrolls");
}

fn max_of(rt: &Runtime<Scene>, key: &str) -> f32 { state(rt, key).max as f32 }

#[test]
fn the_main_scroll_is_in_the_views_points() {
    let mut rt = scene(Scene::Zoomed);
    down(&mut rt, (75.0, 150.0), 200.0);
    let (offset, max) = (offset(&rt, "list") as f32, max_of(&rt, "list"));
    assert!(offset > 0.0, "the wheel scrolled the list");
    assert_eq!(rt.main_scroll(), Some([offset * 2.0, max * 2.0]));
}

#[test]
fn a_shift_keeps_a_held_list_under_the_finger() {
    let mut rt = scene(Scene::Titled);
    let list = rt.tree.nodes.iter().find(|n| n.key.as_deref() == Some("list")).map(|n| n.id).expect("list");
    let vertical = rt.touch_scroll_start(150.0, 300.0);
    rt.touch_scroll_pan(150.0, 300.0, false, vertical);
    rt.touch_scroll_move(list, -100.0);
    assert_eq!(offset(&rt, "list"), 100.0);
    rt.shift_main_scroll(-30.0);
    assert_eq!(offset(&rt, "list"), 70.0, "the bar took 30 points less: the content stays where it was");
    rt.touch_scroll_move(list, -110.0);
    assert_eq!(offset(&rt, "list"), 80.0, "the finger carries on from the shifted origin");
}

#[test]
fn a_shift_leaves_a_list_at_rest_at_its_top_there() {
    let mut rt = scene(Scene::Titled);
    rt.shift_main_scroll(40.0);
    assert_eq!(offset(&rt, "list"), 0.0, "a list at its top stays at its top");
    down(&mut rt, (150.0, 300.0), 20.0);
    rt.shift_main_scroll(-50.0);
    assert_eq!(offset(&rt, "list"), 0.0, "a list at rest isn't shifted past its top");
    down(&mut rt, (150.0, 300.0), 300.0);
    rt.shift_main_scroll(25.0);
    assert_eq!(offset(&rt, "list"), 325.0);
}
