use super::*;
use crate::{Env, GesturePhase, MagnifyEvent, TouchPhase, WheelEvent, el, row};

#[derive(Default)]
struct Canvas {
    pans: Vec<(GesturePhase, f32, f32, bool)>,
    zooms: Vec<f32>,
    beside: usize,
    presses: usize,
}

fn view(_: &Canvas, _: &mut crate::Cx<Canvas>) -> El {
    row()
        .child(
            el().key("canvas")
                .size(200.0, 200.0)
                .on_wheel(|s: &mut Canvas, e, _| { s.pans.push((e.phase, e.dx, e.dy, e.touch)); true })
                .on_magnify(|s: &mut Canvas, e, _| { s.zooms.push(e.delta); true })
                .on_press(|s: &mut Canvas| s.presses += 1)
                .on_secondary_press(|s: &mut Canvas, _, _| s.presses += 1),
        )
        .child(el().key("beside").size(200.0, 200.0).on_wheel(|s: &mut Canvas, _, _| { s.beside += 1; true }))
}

fn pan(rt: &mut Runtime<Canvas>, phase: GesturePhase, at: (f32, f32), moved: (f32, f32)) {
    rt.touch_pan_event(WheelEvent::new(moved.0, moved.1).at(at.0, at.1).phase(phase).precise(true));
}

#[test]
fn two_fingers_that_pinch_and_drift_pan_and_zoom_together() {
    let mut rt = Runtime::new(Canvas::default(), view, (400.0, 200.0), Env::new(true, 1.0));
    rt.frame(0.0);
    pan(&mut rt, GesturePhase::Began, (100.0, 100.0), (0.0, 0.0));
    rt.magnify_event(MagnifyEvent::new(0.0).at(100.0, 100.0).phase(GesturePhase::Began));
    pan(&mut rt, GesturePhase::Moved, (110.0, 105.0), (10.0, 5.0));
    rt.magnify_event(MagnifyEvent::new(0.2).at(110.0, 105.0).phase(GesturePhase::Moved));
    pan(&mut rt, GesturePhase::Moved, (107.0, 105.0), (-3.0, 0.0));
    pan(&mut rt, GesturePhase::Ended, (107.0, 105.0), (0.0, 0.0));
    rt.magnify_event(MagnifyEvent::new(0.0).at(107.0, 105.0).phase(GesturePhase::Ended));
    rt.frame(0.0);
    let s = &rt.state;
    assert!(s.pans.iter().all(|p| p.3), "the pan reaches on_wheel as a touch: {:?}", s.pans);
    let moved = s.pans.iter().fold((0.0, 0.0), |(x, y), p| (x + p.1, y + p.2));
    assert_eq!(moved, (7.0, 5.0), "the content moves as far as the fingers");
    assert_eq!(s.pans.first().map(|p| p.0), Some(GesturePhase::Began));
    assert_eq!(s.pans.last().map(|p| p.0), Some(GesturePhase::Ended));
    assert_eq!(s.zooms, [0.0, 0.2, 0.0], "the pinch zooms alongside");
}

#[test]
fn a_two_finger_pan_stays_with_what_it_began_on() {
    let mut rt = Runtime::new(Canvas::default(), view, (400.0, 200.0), Env::new(true, 1.0));
    rt.frame(0.0);
    pan(&mut rt, GesturePhase::Began, (150.0, 100.0), (0.0, 0.0));
    pan(&mut rt, GesturePhase::Moved, (300.0, 100.0), (150.0, 0.0));
    pan(&mut rt, GesturePhase::Ended, (300.0, 100.0), (0.0, 0.0));
    rt.frame(0.0);
    assert_eq!(rt.state.pans.len(), 3, "the canvas keeps the pan: {:?}", rt.state.pans);
    assert_eq!(rt.state.beside, 0, "the element the fingers drift over hears none of it");
}

#[test]
fn a_slow_two_finger_pan_reaches_the_canvas_once_when_the_host_recognizes_it() {
    let mut rt = Runtime::new(Canvas::default(), view, (400.0, 200.0), Env::new(true, 1.0));
    rt.core_mut().recognize_two_fingers_in_host();
    rt.frame(0.0);
    rt.touch(1, TouchPhase::Started, 80.0, 100.0, 0.0);
    rt.touch(2, TouchPhase::Started, 120.0, 100.0, 0.01);
    for (i, x) in [3.0, 6.0, 9.0, 12.0, 15.0].into_iter().enumerate() {
        let t = 0.02 + i as f64 * 0.01;
        rt.touch(1, TouchPhase::Moved, 80.0 + x, 100.0 + x * 0.5, t);
        rt.touch(2, TouchPhase::Moved, 120.0 + x, 100.0 + x * 0.5, t);
        rt.touch(2, TouchPhase::Moved, 120.0 + x * 1.1, 100.0 + x * 0.5, t);
    }
    rt.touch(1, TouchPhase::Cancelled, 95.0, 107.5, 0.1);
    rt.touch(2, TouchPhase::Cancelled, 136.5, 107.5, 0.1);
    pan(&mut rt, GesturePhase::Began, (100.0, 100.0), (0.0, 0.0));
    pan(&mut rt, GesturePhase::Moved, (120.0, 110.0), (20.0, 10.0));
    pan(&mut rt, GesturePhase::Ended, (120.0, 110.0), (0.0, 0.0));
    rt.frame(0.0);
    let phases: Vec<_> = rt.state.pans.iter().map(|p| p.0).collect();
    assert_eq!(phases, [GesturePhase::Began, GesturePhase::Moved, GesturePhase::Ended], "one pan, from the recognizer alone");
    assert!(rt.state.zooms.is_empty(), "the fingers spreading a little zoom nothing until the host's pinch begins: {:?}", rt.state.zooms);
}

#[test]
fn a_second_finger_still_cancels_the_first_fingers_press_when_the_host_recognizes_two_fingers() {
    let mut rt = Runtime::new(Canvas::default(), view, (400.0, 200.0), Env::new(true, 1.0));
    rt.core_mut().recognize_two_fingers_in_host();
    rt.frame(0.0);
    rt.touch(1, TouchPhase::Started, 80.0, 100.0, 0.0);
    assert!(rt.wake_deadline().is_some(), "one finger arms a long press");
    rt.touch(2, TouchPhase::Started, 120.0, 100.0, 0.01);
    assert_eq!(rt.wake_deadline(), None, "the second finger disarms it");
    rt.touch(2, TouchPhase::Ended, 120.0, 100.0, 0.05);
    rt.touch(1, TouchPhase::Ended, 80.0, 100.0, 0.06);
    rt.frame(1.0);
    assert_eq!(rt.state.presses, 0, "a two-finger tap presses nothing");
    assert!(rt.state.pans.is_empty() && rt.state.zooms.is_empty());
}
