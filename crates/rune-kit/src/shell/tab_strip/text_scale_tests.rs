use rune::{Env, Runtime, col, text};

use super::*;

const TITLE: &str = "Release notes draft";

struct Strip {
    scale: f32,
    viewport: f32,
    probe: bool,
    tabs: usize,
    floor: Option<Option<f32>>,
    selected: String,
}

impl Strip {
    fn new(scale: f32, viewport: f32) -> Self {
        Strip { scale, viewport, probe: false, tabs: 1, floor: None, selected: "a".into() }
    }
}

fn view(s: &Strip, _: &mut rune::Cx<Strip>) -> El {
    let k = Kit::neutral(true).with_text_scale(s.scale);
    if s.probe {
        let font = rune::TextStyle { weight: k.ty.ui_medium.weight, tracking: 0.0, ..k.ty.ui_compact };
        return col().items_start().child(text(TITLE).key("probe").font(font));
    }
    let tabs = (0..s.tabs).map(|i| if i == 0 { TabItem::new("a", TITLE) } else { TabItem::new(format!("t{i}"), TITLE) });
    let mut strip = TabStrip::<Strip>::new("strip", TabOrientation::Horizontal)
        .tabs(tabs)
        .active(Some(&s.selected))
        .viewport_width(Some(s.viewport))
        .on_select(|s: &mut Strip, id, _| s.selected = id.to_owned());
    if let Some(floor) = s.floor {
        strip = strip.title_floor(floor);
    }
    let strip = strip.build(&k);
    col().w(s.viewport).child(strip)
}

fn lay_out(scale: f32, viewport: f32) -> Runtime<Strip> {
    lay_out_strip(Strip::new(scale, viewport))
}

fn lay_out_strip(strip: Strip) -> Runtime<Strip> {
    let viewport = strip.viewport;
    let mut rt = Runtime::new(strip, view, (viewport, 200.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt
}

fn title_width(scale: f32) -> f32 {
    let mut rt = Runtime::new(Strip { probe: true, ..Strip::new(scale, 2000.0) }, view, (2000.0, 200.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rect(&rt, "probe")[2]
}

fn rect(rt: &Runtime<Strip>, key: &str) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no {key}")).rect
}

#[test]
fn a_tab_widens_with_the_text_scale_so_its_title_shows_whole() {
    assert_eq!(rect(&lay_out(1.0, 2000.0), "t-a")[2], 224.0, "the default width at text scale 1 moved");
    for scale in [1.0, 1.6, 2.5] {
        let rt = lay_out(scale, 2000.0);
        let (tab, label) = (rect(&rt, "t-a"), rect(&rt, "label"));
        assert!((tab[2] - 224.0 * scale).abs() < 0.5, "{scale}: the tab is {} wide", tab[2]);
        let needs = title_width(scale);
        assert!(label[2] >= needs, "{scale}: the title has {} of the {needs} it takes", label[2]);
    }
}

#[test]
fn without_a_title_floor_a_tab_keeps_its_density_at_every_text_scale() {
    for scale in [1.0, 1.6, 2.5] {
        for (width, density) in [(40.0, TabDensity::IconOnly), (56.0, TabDensity::Compact), (120.0, TabDensity::Full)] {
            let rt = lay_out_strip(Strip { floor: Some(None), ..Strip::new(scale, width * scale) });
            let tab = rect(&rt, "t-a");
            let shown = TabOverflow::density_for(tab[2] / scale);
            assert_eq!(shown, density, "{scale}: a {} point tab", tab[2]);
            let titled = rt.elements().into_iter().any(|e| e.key.as_deref() == Some("label"));
            assert_eq!(titled, density != TabDensity::IconOnly, "{scale}: a {} point tab's title", tab[2]);
        }
    }
}

fn crowded(scale: f32) -> Runtime<Strip> {
    lay_out_strip(Strip { tabs: 8, ..Strip::new(scale, 400.0) })
}

#[test]
fn at_large_text_crowded_tabs_keep_their_titles_and_the_strip_scrolls() {
    let rt = crowded(1.0);
    let tab = rect(&rt, "t-a");
    assert!((tab[2] - (400.0 - 7.0 * 6.0) / 8.0).abs() < 0.5, "at text scale 1 the tabs share the strip: {}", tab[2]);
    assert_eq!(TabOverflow::density_for(tab[2]), TabDensity::Compact);
    for scale in [1.6, 2.5] {
        let rt = crowded(scale);
        let (tab, viewport, list) = (rect(&rt, "t-a"), rect(&rt, "strip-scroll"), rect(&rt, "strip-list"));
        assert!((tab[2] - TabOverflow::TITLE_FLOOR * scale).abs() < 0.5, "{scale}: a {} point tab", tab[2]);
        assert_eq!(TabOverflow::density_for(tab[2] / scale), TabDensity::Full, "{scale}");
        assert!(rt.elements().into_iter().any(|e| e.key.as_deref() == Some("label")), "{scale}: the titles show");
        assert!(viewport[2] <= 400.0 + 0.5, "{scale}: the strip stays {} wide", viewport[2]);
        assert!(list[2] > viewport[2] * 2.0, "{scale}: the tabs run {} past a {} point strip", list[2], viewport[2]);
    }
}

#[test]
fn the_title_floor_leaves_text_scale_one_alone_and_can_be_turned_off() {
    let policy = TabOverflow::HORIZONTAL_DEFAULT;
    assert_eq!(policy.at_text_scale(1.0).with_title_floor(TabOverflow::TITLE_FLOOR, 1.0), policy);
    let TabOverflow::ShrinkThenScroll { min_width, .. } = policy.at_text_scale(1.6).with_title_floor(TabOverflow::TITLE_FLOOR, 1.6) else { unreachable!() };
    assert!((min_width - TabOverflow::TITLE_FLOOR * 1.6).abs() < 1e-3);
    let rt = lay_out_strip(Strip { tabs: 8, floor: Some(None), ..Strip::new(1.6, 400.0) });
    assert!(rect(&rt, "t-a")[2] < 60.0, "without a floor the tabs share the strip");
}

#[test]
fn end_reaches_the_last_tab_at_large_text_and_scrolls_it_into_view() {
    let mut rt = crowded(2.5);
    for _ in 0..10 {
        rt.frame(1.0 / 60.0);
    }
    let first = rect(&rt, "t-a");
    rt.pointer_move(first[0] + first[2] / 2.0, first[1] + first[3] / 2.0);
    rt.pointer_down(first[0] + first[2] / 2.0, first[1] + first[3] / 2.0);
    rt.pointer_up(first[0] + first[2] / 2.0, first[1] + first[3] / 2.0);
    rt.frame(1.0 / 60.0);
    rt.key(rune::KeyEvent { key: rune::Key::End, mods: rune::Mods::default() });
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
    }
    assert_eq!(rt.state.selected, "t7", "End takes the selection to the last tab");
    let (last, viewport) = (rect(&rt, "t-t7"), rect(&rt, "strip-scroll"));
    assert!(last[0] >= viewport[0] - 0.5 && last[0] + last[2] <= viewport[0] + viewport[2] + 0.5, "the last tab {last:?} shows in {viewport:?}");
}
