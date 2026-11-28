use rune::{Env, Runtime, text};

use super::*;
use crate::IconButton;

const TITLE: &str = "Quarterly planning notes for the design review";
const BUTTONS: usize = 5;

struct Band {
    scale: f32,
}

fn view(s: &Band, _: &mut rune::Cx<Band>) -> El {
    let k = Kit::neutral(true).with_text_scale(s.scale);
    let mut bar = Toolbar::new().leading(text(TITLE).key("title").font(k.ty.ui_medium).truncate().shrink().min_w(0.0));
    for i in 0..BUTTONS {
        bar = bar.trailing(IconButton::new(k.glyph.copy).label(format!("Action {i}")).build(&k).key(format!("b{i}")));
    }
    bar.build(&k)
}

fn lay_out(scale: f32, width: f32) -> Runtime<Band> {
    let mut rt = Runtime::new(Band { scale }, view, (width, 200.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt
}

fn rect(rt: &Runtime<Band>, key: &str) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no {key}")).rect
}

#[test]
fn a_toolbar_title_keeps_its_minimum_and_the_trailing_slot_gives_way_at_every_text_scale() {
    let width = 420.0;
    for (scale, trailing_whole) in [(1.0, true), (1.6, true), (2.5, false)] {
        let natural = rect(&lay_out(scale, 4000.0), "title")[2];
        let rt = lay_out(scale, width);
        let title = rect(&rt, "title");
        let first = rect(&rt, "b0");
        let last = rect(&rt, &format!("b{}", BUTTONS - 1));
        let min = natural.min(LEADING_MIN * scale);
        assert!(title[2] >= min - 0.5, "{scale}: the title shrank to {} under its minimum {min}", title[2]);
        assert!((last[0] + last[2] - (width - PAD)).abs() < 0.5, "{scale}: the last trailing item left the band's end: {last:?}");
        if trailing_whole {
            assert!(title[2] < natural - 0.5, "{scale}: the title should truncate to make room: {title:?}");
            assert!(first[0] >= title[0] + title[2] + 16.0 - 0.5, "{scale}: trailing items cut while the title could still give way: {title:?} {first:?}");
        } else {
            assert!((title[2] - min).abs() < 0.5, "{scale}: the title gave more than its minimum: {title:?}");
            assert!(first[0] < title[0] + title[2], "{scale}: the trailing slot should run off its leading edge: {title:?} {first:?}");
        }
    }
}

#[test]
fn a_toolbar_with_room_lays_out_as_before_at_text_scale_1() {
    let rt = lay_out(1.0, 900.0);
    let title = rect(&rt, "title");
    let natural = rect(&lay_out(1.0, 4000.0), "title")[2];
    assert!((title[2] - natural).abs() < 0.5, "the title truncated with room to spare: {title:?}");
    assert_eq!(title[0], PAD);
    let last = rect(&rt, &format!("b{}", BUTTONS - 1));
    assert!((last[0] + last[2] - (900.0 - PAD)).abs() < 0.5, "the trailing items moved off the band's end: {last:?}");
    for i in 1..BUTTONS {
        let (a, b) = (rect(&rt, &format!("b{}", i - 1)), rect(&rt, &format!("b{i}")));
        assert!((b[0] - (a[0] + a[2] + 4.0)).abs() < 0.5, "trailing items lost their 4 point gap: {a:?} {b:?}");
    }
}
