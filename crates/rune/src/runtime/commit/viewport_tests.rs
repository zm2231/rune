use crate::{Cx, Direction, El, Env, Runtime, ScrollSpec, col, el, row};

fn rect<S>(rt: &Runtime<S>, key: &str) -> [f32; 4] {
    rt.tree.nodes.iter().find(|n| n.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no {key}")).target
}

fn laid_out<S: 'static>(state: S, view: fn(&S, &mut Cx<S>) -> El, w: f32) -> Runtime<S> {
    let mut rt = Runtime::new(state, view, (w, 400.0), Env::default());
    rt.frame(0.0);
    rt
}

fn assert_near(got: f32, want: f32, what: &str) {
    assert!((got - want).abs() < 0.01, "{what}: {got} vs {want}");
}

fn vertical() -> ScrollSpec {
    ScrollSpec { stick_bottom: false }
}

#[test]
fn content_centred_on_a_horizontal_viewport_starts_at_its_left_edge_once_it_overflows() {
    fn view(w: &f32, _: &mut Cx<f32>) -> El {
        let strip = row().key("strip").w_full().scroll_x().justify_center().child(el().key("tabs").w(*w).h(20.0));
        let center = col().key("center").w_full().scroll_x().center().child(el().key("tabs2").w(*w).h(20.0));
        col().w_full().child(strip).child(center)
    }
    let fits = laid_out(200.0, view, 300.0);
    assert_near(rect(&fits, "tabs")[0], 50.0, "centred while it fits");
    assert_near(rect(&fits, "tabs2")[0], 50.0, ".center() centred while it fits");
    let wide = laid_out(500.0, view, 300.0);
    assert_near(rect(&wide, "tabs")[0], 0.0, "overflowing content starts at the left edge");
    assert_near(rect(&wide, "tabs2")[0], 0.0, ".center() overflowing content starts at the left edge");
}

#[test]
fn content_centred_or_ended_on_a_vertical_viewport_starts_at_its_top_once_it_overflows() {
    fn view(h: &f32, _: &mut Cx<f32>) -> El {
        let centred = col().key("centred").w_full().h(100.0).scroll(vertical()).justify_center().child(el().key("a").w(50.0).h(*h));
        let ended = col().key("ended").w_full().h(100.0).scroll(vertical()).justify_end().child(el().key("b").w(50.0).h(*h));
        col().w_full().child(centred).child(ended)
    }
    let fits = laid_out(60.0, view, 300.0);
    assert_near(rect(&fits, "a")[1], 20.0, "centred while it fits");
    assert_near(rect(&fits, "b")[1], 100.0 + 40.0, "at the bottom while it fits");
    let tall = laid_out(300.0, view, 300.0);
    assert_near(rect(&tall, "a")[1], rect(&tall, "centred")[1], "overflowing centred content starts at the top");
    assert_near(rect(&tall, "b")[1], rect(&tall, "ended")[1], "overflowing end-aligned content starts at the top");
}

#[test]
fn a_viewport_scrolling_both_ways_keeps_overflowing_centred_content_reachable_on_both_axes() {
    fn view(size: &(f32, f32), _: &mut Cx<(f32, f32)>) -> El {
        col().key("both").w_full().h(100.0).scroll_xy().center().child(el().key("c").w(size.0).h(size.1))
    }
    let fits = laid_out((100.0, 40.0), view, 300.0);
    assert_near(rect(&fits, "c")[0], 100.0, "centred across while it fits");
    let big = laid_out((500.0, 300.0), view, 300.0);
    assert_eq!(&rect(&big, "c")[..2], &rect(&big, "both")[..2], "overflowing content starts at the top left");
}

#[test]
fn a_childs_own_centring_across_a_scrolled_cross_axis_is_safe() {
    fn view(h: &f32, _: &mut Cx<f32>) -> El {
        col().w_full().child(row().key("rows").w_full().h(100.0).scroll(vertical()).child(el().key("tall").w(50.0).h(*h).self_center()).child(el().key("end").w(50.0).h(*h).self_end()))
    }
    let fits = laid_out(60.0, view, 300.0);
    assert_near(rect(&fits, "tall")[1], 20.0, "self_center while it fits");
    assert_near(rect(&fits, "end")[1], 40.0, "self_end while it fits");
    let tall = laid_out(300.0, view, 300.0);
    assert_near(rect(&tall, "tall")[1], 0.0, "overflowing self_center starts at the top");
    assert_near(rect(&tall, "end")[1], 0.0, "overflowing self_end starts at the top");
}

#[test]
fn right_to_left_overflowing_centred_content_starts_at_the_right_edge() {
    fn view(w: &f32, _: &mut Cx<f32>) -> El {
        col().w_full().child(row().key("strip").dir(Direction::Rtl).w_full().scroll_x().justify_center().child(el().key("tabs").w(*w).h(20.0)))
    }
    let fits = laid_out(200.0, view, 300.0);
    assert_near(rect(&fits, "tabs")[0], 50.0, "centred while it fits");
    let wide = laid_out(500.0, view, 300.0);
    assert_near(rect(&wide, "tabs")[2], 300.0, "overflowing content starts at the right edge");
}

#[test]
fn a_full_width_centring_column_in_a_horizontal_viewport_centres_its_tabs_or_starts_them_at_the_left() {
    fn view(w: &f32, _: &mut Cx<f32>) -> El {
        let tabs = row().key("tabs").child(el().w(*w / 2.0).h(28.0)).child(el().w(*w / 2.0).h(28.0));
        col().key("strip").w_full().scroll_x().scroll_edge_fade(24.0).child(col().key("inner").w_full().items_center().pt(14.0).pb(10.0).child(tabs))
    }
    let fits = laid_out(200.0, view, 300.0);
    assert_near(rect(&fits, "tabs")[0], 50.0, "centred while they fit");
    let wide = laid_out(800.0, view, 300.0);
    assert_near(rect(&wide, "inner")[2], 800.0, "the column grows to the tabs");
    assert_near(rect(&wide, "tabs")[0], 0.0, "overflowing tabs start at the left edge");
}

#[cfg(debug_assertions)]
fn warns<S: 'static>(state: S, view: fn(&S, &mut Cx<S>) -> El) -> bool {
    let _warnings = super::super::layout_diagnostics::scope();
    laid_out(state, view, 300.0);
    super::super::layout_diagnostics::warned_at("unreachable:")
}

#[cfg(debug_assertions)]
#[test]
fn content_a_fixed_or_viewport_sized_box_pushes_before_the_start_warns_in_debug_builds() {
    fn fixed_width(_: &(), _: &mut Cx<()>) -> El {
        col().w_full().child(row().w_full().scroll_x().child(col().w(300.0).items_center().child(el().key("wide").w(500.0).h(20.0))))
    }
    fn viewport_height(_: &(), _: &mut Cx<()>) -> El {
        col().w_full().child(col().w_full().h(100.0).scroll(vertical()).child(col().w_full().h_full().justify_center().child(el().h(300.0))))
    }
    fn clipped_crop(_: &(), _: &mut Cx<()>) -> El {
        let thumb = row().size(40.0, 40.0).center().clip_rounded(4.0).child(el().size(80.0, 80.0));
        col().w_full().child(col().w_full().h(100.0).scroll(vertical()).child(col().w_full().child(thumb).child(el().h(300.0))))
    }
    fn safe(_: &(), _: &mut Cx<()>) -> El {
        col().w_full().child(row().w_full().scroll_x().justify_center().child(el().w(500.0).h(20.0)))
    }
    fn right_to_left(_: &(), _: &mut Cx<()>) -> El {
        col().w_full().child(row().dir(Direction::Rtl).w_full().scroll_x().child(el().w(600.0).h(20.0)))
    }
    fn right_to_left_fixed_width(_: &(), _: &mut Cx<()>) -> El {
        col().w_full().child(row().dir(Direction::Rtl).w_full().scroll_x().child(col().w(300.0).items_center().child(el().w(500.0).h(20.0))))
    }
    assert!(warns((), fixed_width), "a fixed-width box centring wider content");
    assert!(warns((), right_to_left_fixed_width), "the same right to left");
    assert!(!warns((), right_to_left), "right-to-left content overflowing to the left");
    assert!(warns((), viewport_height), "a viewport-tall box centring taller content");
    assert!(!warns((), clipped_crop), "a centred crop inside a clipped box");
    assert!(!warns((), safe), "a viewport's own centring is safe");
}
