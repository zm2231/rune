use rune::{Env, Runtime, col};

use super::*;

#[derive(Default)]
struct App {
    pressed: Vec<usize>,
    rtl: bool,
    reveal: Option<usize>,
}

fn view(s: &App, cx: &mut rune::Cx<App>) -> El {
    cx.web_focus();
    let k = Kit::neutral(true);
    let items = (0..10).map(|i| el().key(format!("item{i}")).role("button").w(100.0).h(28.0).on_press(move |s: &mut App| s.pressed.push(i)));
    let viewport = row().scroll_x().child(row().key("list").children(items));
    let strip = ScrollOverflow::new("strip", viewport).reveal(s.reveal.map(|i| format!("item{i}"))).build(&k).w_full().h(28.0);
    col().child(if s.rtl { strip.dir(rune::Direction::Rtl) } else { strip })
}

fn app(reduced_motion: bool) -> Runtime<App> {
    mount(App::default(), reduced_motion)
}

fn mount(state: App, reduced_motion: bool) -> Runtime<App> {
    let mut rt = Runtime::new(state, view, (300.0, 100.0), Env::new(true, 1.0));
    rt.set_system_reduced_motion(reduced_motion);
    settle(&mut rt);
    rt
}

fn settle(rt: &mut Runtime<App>) {
    for _ in 0..90 {
        rt.frame(1.0 / 60.0);
    }
}

fn rect(rt: &Runtime<App>, key: &str) -> [f32; 4] {
    rt.presented(key).unwrap_or_else(|| panic!("no {key}")).0
}

fn opacity(rt: &Runtime<App>, key: &str) -> f32 {
    rt.presented(key).unwrap_or_else(|| panic!("no {key}")).1
}

fn press(rt: &mut Runtime<App>, key: &str) {
    let r = rect(rt, key);
    let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn scrolled(rt: &Runtime<App>) -> f32 {
    rect(rt, "strip")[0] - rect(rt, "list")[0]
}

#[test]
fn a_chevron_shows_only_over_an_edge_with_content_past_it() {
    let mut rt = app(false);
    assert_eq!(opacity(&rt, "overflow-start"), 0.0, "nothing is past the start");
    assert!(opacity(&rt, "overflow-end") > 0.99, "the end has 700 points past it");
    press(&mut rt, "overflow-end");
    settle(&mut rt);
    assert!(opacity(&rt, "overflow-start") > 0.99, "after a page the start has content past it");
    for _ in 0..2 {
        press(&mut rt, "overflow-end");
        settle(&mut rt);
    }
    assert_eq!(scrolled(&rt), 700.0, "three pages reach the end");
    assert_eq!(opacity(&rt, "overflow-end"), 0.0, "at the end nothing is past it");
}

#[test]
fn a_hidden_chevron_takes_no_presses() {
    let mut rt = app(false);
    press(&mut rt, "overflow-start");
    settle(&mut rt);
    assert_eq!(rt.state.pressed, [0], "the press reaches the item under the hidden chevron");
    assert_eq!(scrolled(&rt), 0.0, "and scrolls nothing");
}

#[test]
fn pressing_a_chevron_pages_toward_its_edge() {
    let mut rt = app(false);
    press(&mut rt, "overflow-end");
    let first = scrolled(&rt);
    settle(&mut rt);
    let page = 300.0 - 2.0 * WIDTH;
    assert!((scrolled(&rt) - page).abs() < 0.5, "one page is the viewport less both chevrons: {}", scrolled(&rt));
    assert!(first < page, "the page glides: {first} on its first frame");
    assert!(rt.state.pressed.is_empty(), "the chevron's press doesn't reach the item under it");
    press(&mut rt, "overflow-start");
    settle(&mut rt);
    assert_eq!(scrolled(&rt), 0.0, "and back to the start");
}

#[test]
fn under_reduced_motion_a_page_jumps() {
    let mut rt = app(true);
    press(&mut rt, "overflow-end");
    assert!((scrolled(&rt) - (300.0 - 2.0 * WIDTH)).abs() < 0.5, "landed on the first frame: {}", scrolled(&rt));
}

#[test]
fn presses_during_a_glide_page_on_from_where_the_last_was_headed() {
    let mut rt = app(false);
    press(&mut rt, "overflow-end");
    press(&mut rt, "overflow-end");
    settle(&mut rt);
    assert!((scrolled(&rt) - 2.0 * (300.0 - 2.0 * WIDTH)).abs() < 0.5, "two pages: {}", scrolled(&rt));
}

#[test]
fn the_chevrons_are_no_tab_stops_even_where_every_button_is() {
    let rt = app(false);
    let els = rt.elements();
    assert!(els.iter().any(|e| e.key.as_deref() == Some("item0") && e.focusable), "web focus makes a pressable button a Tab stop");
    for key in ["overflow-start", "overflow-end"] {
        let chevron = els.iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no {key}"));
        assert!(!chevron.focusable, "{key} is a Tab stop");
    }
}

#[test]
fn right_to_left_the_start_chevron_stands_at_the_right() {
    let rt = mount(App { rtl: true, ..App::default() }, false);
    let (start, end, strip) = (rect(&rt, "overflow-start"), rect(&rt, "overflow-end"), rect(&rt, "strip"));
    assert!((start[0] + start[2] - (strip[0] + strip[2])).abs() < 0.5 && (end[0] - strip[0]).abs() < 0.5, "start {start:?} end {end:?} in {strip:?}");
    assert_eq!(opacity(&rt, "overflow-start"), 0.0, "nothing is past the start");
    assert!(opacity(&rt, "overflow-end") > 0.99, "the end, on the left, has content past it");
}

#[test]
fn holding_a_chevron_keeps_paging_until_let_go() {
    let mut rt = app(false);
    let r = rect(&rt, "overflow-end");
    let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
    }
    let page = 300.0 - 2.0 * WIDTH;
    assert!(scrolled(&rt) > 2.0 * page - 0.5, "a second held for paging goes on past two pages: {}", scrolled(&rt));
    rt.pointer_up(x, y);
    settle(&mut rt);
    let landed = scrolled(&rt);
    settle(&mut rt);
    assert_eq!(scrolled(&rt), landed, "letting go stops it");
    assert!(rt.state.pressed.is_empty());
}

#[test]
fn a_view_opens_on_the_element_it_reveals() {
    let rt = mount(App { reveal: Some(8), ..App::default() }, false);
    let (item, strip) = (rect(&rt, "item8"), rect(&rt, "strip"));
    assert!(item[0] >= strip[0] - 0.5 && item[0] + item[2] <= strip[0] + strip[2] + 0.5, "item8 {item:?} shows in {strip:?}");
    assert!(opacity(&rt, "overflow-start") > 0.99, "and the start chevron says what's before it");
}

#[test]
fn a_window_grown_to_fit_drops_the_chevrons_and_the_scroll() {
    let mut rt = app(false);
    press(&mut rt, "overflow-end");
    settle(&mut rt);
    assert!(scrolled(&rt) > 0.0);
    rt.set_viewport((1200.0, 100.0), 1.0);
    settle(&mut rt);
    assert_eq!(scrolled(&rt), 0.0, "everything fits, so nothing is scrolled");
    assert_eq!((opacity(&rt, "overflow-start"), opacity(&rt, "overflow-end")), (0.0, 0.0), "and no chevron shows");
    press(&mut rt, "item9");
    assert_eq!(rt.state.pressed, [9], "the hidden chevron leaves the last item its presses");
}
