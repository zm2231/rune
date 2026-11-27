use rune::{Cx, El, Env, GesturePhase, MagnifyEvent, Mods, Runtime, TouchPhase, WheelEvent, el};
use rune_kit::{Canvas, ImageZoom, Kit, Preview, Stage, StageZoom, ZoomPan};

struct App {
    pan: ZoomPan,
    image: ImageZoom,
    stage: StageZoom,
}

const WINDOW: (f32, f32) = (900.0, 600.0);

fn pane(s: &App, _: &mut Cx<App>) -> El {
    el().child(Canvas::new(|s: &mut App| &mut s.pan, [1200.0, 800.0], el().size(1200.0, 800.0)).build(&s.pan, [600.0, 400.0]))
}

fn preview(s: &App, _: &mut Cx<App>) -> El {
    let image = Preview::new(|s: &mut App| &mut s.image, [800.0, 600.0], el().key("picture"));
    el().w_full().h_full().child(image.build(&s.image, &Kit::neutral(true), [WINDOW.0, WINDOW.1], false))
}

fn stage(s: &App, _: &mut Cx<App>) -> El {
    let image = Stage::new(|s: &mut App| &mut s.stage, [800.0, 600.0], el().key("picture"));
    el().child(image.build(&s.stage, &Kit::neutral(true), [600.0, 400.0]))
}

fn run(view: fn(&App, &mut Cx<App>) -> El) -> Runtime<App> {
    let app = App { pan: ZoomPan::new(), image: ImageZoom::new(), stage: StageZoom::default() };
    let mut rt = Runtime::new(app, view, WINDOW, Env::new(true, 1.0));
    rt.frame(0.0);
    rt
}

fn pinch(rt: &mut Runtime<App>, at: (f32, f32)) {
    for (delta, phase) in [(0.0, GesturePhase::Began), (0.25, GesturePhase::Moved), (0.2, GesturePhase::Moved), (0.0, GesturePhase::Ended)] {
        rt.magnify_event(MagnifyEvent::new(delta).at(at.0, at.1).phase(phase));
        rt.frame(0.0);
    }
}

#[test]
fn a_pinch_zooms_a_zoom_pan_canvas() {
    let mut rt = run(pane);
    let before = rt.state.pan.scale();
    pinch(&mut rt, (300.0, 200.0));
    let after = rt.state.pan.scale();
    assert!((after / before - 1.5).abs() < 1e-3, "the canvas grows by the pinch: {before} to {after}");
}

#[test]
fn a_pinch_zooms_an_image_preview() {
    let mut rt = run(preview);
    let before = rt.state.image.view().scale;
    pinch(&mut rt, (WINDOW.0 / 2.0, WINDOW.1 / 2.0));
    assert!(rt.state.image.view().scale > before * 1.2, "the preview zooms: {before} to {}", rt.state.image.view().scale);
}

#[test]
fn a_pinch_zooms_an_image_stage() {
    let mut rt = run(stage);
    pinch(&mut rt, (300.0, 200.0));
    assert!(rt.state.stage.zoomed(), "the stage zooms in");
}

#[test]
fn two_fingers_on_a_touchscreen_zoom_a_canvas() {
    let mut rt = run(pane);
    let before = rt.state.pan.scale();
    rt.touch(1, TouchPhase::Started, 200.0, 200.0, 0.0);
    rt.touch(2, TouchPhase::Started, 300.0, 200.0, 0.01);
    rt.touch(2, TouchPhase::Moved, 400.0, 200.0, 0.02);
    rt.touch(2, TouchPhase::Ended, 400.0, 200.0, 0.03);
    rt.touch(1, TouchPhase::Ended, 200.0, 200.0, 0.04);
    rt.frame(1.0 / 60.0);
    assert!(rt.state.pan.scale() > before * 1.2, "spreading fingers zoom in: {before} to {}", rt.state.pan.scale());
}

#[test]
fn ctrl_wheel_handed_over_as_a_pinch_zooms_as_the_wheel_does() {
    let ctrl = Mods { ctrl: true, ..Mods::default() };
    let views = [
        (pane as fn(&App, &mut Cx<App>) -> El, (|s: &App| s.pan.scale()) as fn(&App) -> f32, (300.0, 200.0)),
        (preview, |s: &App| s.image.view().scale, (WINDOW.0 / 2.0, WINDOW.1 / 2.0)),
    ];
    let wheels = [(20.0, 20.0 / 200.0), (12.0, 12.0 / 200.0), (120.0, 1.2_f32.powi(3) - 1.0), (10.0, 1.2_f32.powf(0.25) - 1.0)];
    for ((view, scale, at), (dy, delta)) in views.into_iter().flat_map(|v| wheels.map(|w| (v, w))) {
        let mut wheel = run(view);
        wheel.wheel_event(WheelEvent::new(0.0, dy).at(at.0, at.1).mods(ctrl));
        wheel.frame(0.0);
        let mut pinch = run(view);
        let mut ev = MagnifyEvent::new(delta).at(at.0, at.1);
        ev.wheel = true;
        ev.wheel_dy = dy;
        pinch.magnify_event(ev);
        pinch.frame(0.0);
        let (by_wheel, by_pinch) = (scale(&wheel.state), scale(&pinch.state));
        assert!(by_wheel > scale(&run(view).state), "Ctrl + wheel zooms in");
        assert!((by_wheel - by_pinch).abs() < 1e-4, "the same zoom either way for dy {dy}: {by_wheel} by wheel, {by_pinch} as a pinch");
    }
}

fn two_fingers(rt: &mut Runtime<App>, at: (f32, f32), steps: &[(f32, [f32; 2])]) {
    let (mut t, [mut x, mut y], mut half) = (0.01, [at.0, at.1], 50.0);
    rt.touch(1, TouchPhase::Started, x - half, y, 0.0);
    rt.touch(2, TouchPhase::Started, x + half, y, t);
    for &(spread, [dx, dy]) in steps {
        (t, x, y, half) = (t + 0.01, x + dx, y + dy, half + spread);
        rt.touch(1, TouchPhase::Moved, x - half, y, t);
        rt.touch(2, TouchPhase::Moved, x + half, y, t);
        rt.frame(0.0);
    }
    rt.touch(2, TouchPhase::Ended, x + half, y, t + 0.01);
    rt.touch(1, TouchPhase::Ended, x - half, y, t + 0.02);
    rt.frame(1.0 / 60.0);
}

#[test]
fn two_fingers_moving_together_pan_a_zoomed_canvas() {
    let mut rt = run(pane);
    pinch(&mut rt, (300.0, 200.0));
    pinch(&mut rt, (300.0, 200.0));
    let (scale, [x, y]) = (rt.state.pan.scale(), rt.state.pan.to_content([300.0, 200.0]));
    two_fingers(&mut rt, (300.0, 200.0), &[(0.0, [3.0, 3.0]); 20]);
    let [x2, y2] = rt.state.pan.to_content([300.0, 200.0]);
    let along = 60.0 * (1.0 - 8.0 / 7200.0_f32.sqrt());
    assert!(((x - x2) * scale - along).abs() < 0.1 && ((y - y2) * scale - along).abs() < 0.1, "the content follows the fingers past the slop: ({x}, {y}) to ({x2}, {y2})");
}

#[test]
fn a_two_finger_pan_that_begins_with_the_pinch_pans_the_stage_once_it_zooms() {
    let pan = |drift: f32| {
        let mut rt = run(stage);
        let steps: Vec<_> = (0..20).map(|_| (4.0, [0.0, 0.0])).chain((0..10).map(|_| (0.0, [drift, 0.0]))).collect();
        two_fingers(&mut rt, (300.0, 200.0), &steps);
        assert!(rt.state.stage.zoomed(), "the spread zooms the stage in");
        rt.state.stage.pan()
    };
    let (still, drifted) = (pan(0.0), pan(3.0));
    assert!((drifted[0] - still[0] - 30.0).abs() < 0.1, "the stage follows the fingers 30 points: {still:?} to {drifted:?}");
}
