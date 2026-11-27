use crate::render::KIND_GLYPH;
use crate::{Cx, El, Env, Icon, Runtime, el, icon};

static ARC: Icon = Icon {
    name: "loop-turn-arc",
    view: 16.0,
    body: r#"<path d="M8 2A6 6 0 0 1 13.258 10.89" stroke="currentColor" stroke-width="2" stroke-linecap="round" fill="none"/>"#,
};

fn loader(fps: &f32, _: &mut Cx<f32>) -> El {
    el().size(40.0, 40.0).child(icon(&ARC, 16.0).key("arc").looping(rune_motion::LoopSpec::rotation(0.9).fps(*fps)))
}

fn turning(fps: f32, frames: usize) -> Runtime<f32> {
    let mut rt = Runtime::new(fps, loader, (40.0, 40.0), Env::new(true, 2.0));
    rt.wall_start = Some(crate::time::now());
    rt.frame(0.0);
    for _ in 0..frames {
        rt.frame(1.0 / 120.0);
    }
    rt
}

fn turns(rt: &Runtime<f32>) -> Vec<f32> {
    rt.instances.iter().filter(|q| q.params[1] == KIND_GLYPH).map(|q| q.params[0]).collect()
}

#[test]
fn a_turning_icon_steps_without_a_paint_and_lands_where_a_frame_would() {
    let display = rune_motion::Cycle::DISPLAY_RATE;
    let mut stepped = turning(display, 3);
    let mut framed = turning(display, 3);
    let paints = stepped.paint_count();
    let patched = stepped.core_mut().loop_step(1.0 / 120.0).expect("a rotation takes a loop-only step");
    framed.frame(1.0 / 120.0);
    assert_eq!(patched.len(), 1, "only the turning glyph changes");
    assert_eq!(stepped.paint_count(), paints, "the step paints nothing");
    let (step, frame) = (turns(&stepped), turns(&framed));
    assert_eq!(step.len(), 1);
    assert_eq!(step.len(), frame.len());
    assert!((step[0] - frame[0]).abs() < 1e-4, "step turned to {}, a frame to {}", step[0], frame[0]);
    assert_ne!(step[0], turns(&turning(display, 3))[0], "the step moved the turn on");
}

#[test]
fn a_rotation_with_a_rate_wakes_at_that_rate_instead_of_every_display_frame() {
    let mut rt = Runtime::new(40.0, loader, (40.0, 40.0), Env::new(true, 2.0));
    rt.wall_start = Some(crate::time::now());
    rt.frame.set_test_clock(crate::ManualClock::new(120.0));
    let mut animating = rt.frame(0.0);
    for _ in 0..3 {
        let clock = rt.frame.test_clock_mut().unwrap();
        clock.seek(clock.now() + 1.0 / 120.0);
        animating |= rt.frame(1.0 / 120.0);
    }
    assert!(!animating, "a stepped rotation asks for no display-rate frames");
    let due = rt.wake_deadline().expect("it wakes for its next step");
    assert!(due - rt.clock <= 1.0 / 40.0 + 1e-9, "next step {} s away", due - rt.clock);
    let before = turns(&rt);
    rt.frame.test_clock_mut().unwrap().seek(due);
    let dt = due - rt.clock;
    let patched = rt.core_mut().loop_step(dt).expect("its wake takes a loop-only step");
    assert_eq!(patched.len(), 1);
    assert_ne!(turns(&rt), before, "the step moved the turn on");
    assert!(rt.wake_deadline().is_some_and(|next| next > due), "the step armed the next wake");
}
