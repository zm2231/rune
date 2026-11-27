use super::*;
use crate::{Align, ScreenEdge};

const PHONE: (f32, f32) = (393.0, 852.0);
const NOTCHED: [f32; 4] = [59.0, 0.0, 34.0, 0.0];

fn pill(edge: ScreenEdge, align: Align) -> Placement {
    Placement { size: (200.0, 44.0), edge, margin: 8.0, align, inset: 12.0 }
}

#[test]
fn a_top_edge_overlay_sits_below_the_safe_area_centred_along_it() {
    assert_eq!(frame(pill(ScreenEdge::Top, Align::Center), None, PHONE, NOTCHED), [96.5, 67.0, 200.0, 44.0]);
}

#[test]
fn a_bottom_edge_overlay_sits_above_the_home_indicator() {
    assert_eq!(frame(pill(ScreenEdge::Bottom, Align::Center), None, PHONE, NOTCHED), [96.5, 852.0 - 34.0 - 8.0 - 44.0, 200.0, 44.0]);
}

#[test]
fn edge_insets_measure_from_the_safe_area_sides_in_landscape() {
    let landscape = (852.0, 393.0);
    let safe = [0.0, 59.0, 21.0, 59.0];
    assert_eq!(frame(pill(ScreenEdge::Top, Align::Start), None, landscape, safe), [59.0 + 12.0, 8.0, 200.0, 44.0]);
    assert_eq!(frame(pill(ScreenEdge::Top, Align::End), None, landscape, safe), [852.0 - 59.0 - 12.0 - 200.0, 8.0, 200.0, 44.0]);
}

#[test]
fn an_edge_placement_option_overrides_the_spec() {
    let edge = OverlayPlacement::Edge { edge: ScreenEdge::Bottom, margin: 0.0, align: Align::Start, inset: 0.0 };
    assert_eq!(frame(pill(ScreenEdge::Top, Align::Center), Some(edge), PHONE, NOTCHED), [0.0, 852.0 - 34.0 - 44.0, 200.0, 44.0]);
}

#[test]
fn rect_and_full_screen_ignore_the_safe_area() {
    let rect = OverlayPlacement::Rect { x: 10.0, y: 20.0, width: 100.0, height: 50.0 };
    assert_eq!(frame(pill(ScreenEdge::Top, Align::Center), Some(rect), PHONE, NOTCHED), [10.0, 20.0, 100.0, 50.0]);
    assert_eq!(frame(pill(ScreenEdge::Top, Align::Center), Some(OverlayPlacement::FullScreen), PHONE, NOTCHED), [0.0, 0.0, 393.0, 852.0]);
}

#[test]
fn touches_pass_through_unless_the_overlay_takes_them() {
    let regions = [[10.0, 10.0, 50.0, 30.0]];
    assert!(!takes(OverlayInput::Legacy, false, &regions, 20.0, 20.0));
    assert!(takes(OverlayInput::Legacy, true, &[], 1.0, 1.0));
    assert!(!takes(OverlayInput::None, true, &regions, 20.0, 20.0));
    assert!(takes(OverlayInput::All, false, &[], 1.0, 1.0));
    assert!(takes(OverlayInput::Regions, false, &regions, 20.0, 20.0));
    assert!(!takes(OverlayInput::Regions, false, &regions, 60.0, 20.0));
}

fn run(seq: &mut Seq, state: &mut State, event: Event) -> Vec<Effect> {
    step(seq, state, event).into_iter().collect()
}

#[test]
fn an_overlay_shows_and_hides_at_once() {
    let (mut seq, mut state) = (Seq::default(), State::default());
    assert_eq!(run(&mut seq, &mut state, Event::Show(Duration::ZERO)), [Effect::Attach { alpha: 1.0 }]);
    assert!(run(&mut seq, &mut state, Event::Show(Duration::ZERO)).is_empty());
    assert_eq!(run(&mut seq, &mut state, Event::Hide(Duration::ZERO)), [Effect::Detach]);
    assert_eq!(state, State::Hidden);
    assert!(run(&mut seq, &mut state, Event::Hide(Duration::ZERO)).is_empty());
}

#[test]
fn a_fade_out_detaches_when_it_ends_and_a_show_during_it_keeps_the_overlay() {
    let (mut seq, mut state) = (Seq::default(), State::default());
    let fade = Duration::from_millis(200);
    let shown = run(&mut seq, &mut state, Event::Show(fade));
    assert!(matches!(shown[..], [Effect::Attach { alpha: 0.0 }, Effect::Fade { alpha: 1.0, .. }]));
    let [Effect::Fade { alpha: 0.0, attempt: out, .. }] = run(&mut seq, &mut state, Event::Hide(fade))[..] else { panic!("no fade out") };
    assert!(matches!(run(&mut seq, &mut state, Event::Show(fade))[..], [Effect::Fade { alpha: 1.0, .. }]));
    assert!(run(&mut seq, &mut state, Event::Faded(out)).is_empty());
    assert_eq!(state, State::Shown);
    let [Effect::Fade { attempt, .. }] = run(&mut seq, &mut state, Event::Hide(fade))[..] else { panic!("no fade out") };
    assert_eq!(run(&mut seq, &mut state, Event::Faded(attempt)), [Effect::Detach]);
}
