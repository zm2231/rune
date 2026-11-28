use rune::{Env, Runtime, col, text};

use super::*;

const TITLE: &str = "Release notes draft";

struct Strip {
    scale: f32,
    viewport: f32,
    probe: bool,
}

fn view(s: &Strip, _: &mut rune::Cx<Strip>) -> El {
    let k = Kit::neutral(true).with_text_scale(s.scale);
    if s.probe {
        let font = rune::TextStyle { weight: k.ty.ui_medium.weight, tracking: 0.0, ..k.ty.ui_compact };
        return col().items_start().child(text(TITLE).key("probe").font(font));
    }
    let strip = TabStrip::<Strip>::new("strip", TabOrientation::Horizontal)
        .tabs([TabItem::new("a", TITLE)])
        .viewport_width(Some(s.viewport))
        .build(&k);
    col().w(s.viewport).child(strip)
}

fn lay_out(scale: f32, viewport: f32) -> Runtime<Strip> {
    let mut rt = Runtime::new(Strip { scale, viewport, probe: false }, view, (viewport, 200.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt
}

fn title_width(scale: f32) -> f32 {
    let mut rt = Runtime::new(Strip { scale, viewport: 2000.0, probe: true }, view, (2000.0, 200.0), Env::new(true, 1.0));
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
fn a_tab_keeps_its_density_at_every_text_scale() {
    for scale in [1.0, 1.6, 2.5] {
        for (width, density) in [(40.0, TabDensity::IconOnly), (56.0, TabDensity::Compact), (120.0, TabDensity::Full)] {
            let rt = lay_out(scale, width * scale);
            let tab = rect(&rt, "t-a");
            let shown = TabOverflow::density_for(tab[2] / scale);
            assert_eq!(shown, density, "{scale}: a {} point tab", tab[2]);
            let titled = rt.elements().into_iter().any(|e| e.key.as_deref() == Some("label"));
            assert_eq!(titled, density != TabDensity::IconOnly, "{scale}: a {} point tab's title", tab[2]);
        }
    }
}
