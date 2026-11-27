use crate::render::KIND_GLYPH;
use crate::{Cx, El, Env, Icon, Runtime, el, icon};

static ARC: Icon = Icon {
    name: "loop-turn-arc",
    view: 16.0,
    body: r#"<path d="M8 2A6 6 0 0 1 13.258 10.89" stroke="currentColor" stroke-width="2" stroke-linecap="round" fill="none"/>"#,
};

#[derive(Default)]
struct Spin {
    fps: f32,
    dots: bool,
    scrolled: bool,
    faded: bool,
    polls: usize,
    next: f64,
}

fn loader(spin: &Spin, _: &mut Cx<Spin>) -> El {
    if spin.dots {
        let stops: &'static [(f32, f32)] = &[(0.0, 0.2), (0.5, 1.0), (1.0, 0.2)];
        let dot = |k: usize| el().key(format!("dot{k}")).size(4.0, 4.0).fill(crate::Color::WHITE).a11y_hidden()
            .cycle(rune_motion::Cycle { period: 1.8, phase: k as f32 / 3.0, stops, still: 0.5, fps: spin.fps });
        if spin.faded {
            return crate::col().size(40.0, 40.0).child(el().key("row").size(40.0, 10.0).opacity(0.5).child(dot(0)))
                .child(el().key("dim").size(40.0, 10.0).opacity(0.5).child(el().size(20.0, 8.0).fill(crate::Color::WHITE)).child(dot(1)));
        }
        if spin.scrolled {
            return crate::col().size(40.0, 40.0).scroll(crate::ScrollSpec { stick_bottom: false })
                .child(el().size(40.0, 10.0).child(dot(0)))
                .child(el().size(40.0, 400.0))
                .child(el().size(40.0, 10.0).child(dot(1)));
        }
        return el().size(40.0, 40.0).child(dot(0)).child(dot(1)).child(dot(2));
    }
    el().size(40.0, 40.0).child(icon(&ARC, 16.0).key("arc").looping(rune_motion::LoopSpec::rotation(0.9).fps(spin.fps)))
}

fn turns(rt: &Runtime<Spin>) -> Vec<f32> {
    rt.instances.iter().filter(|q| q.params[1] == KIND_GLYPH).map(|q| q.params[0]).collect()
}

#[test]
fn a_turning_icon_steps_without_a_paint_and_lands_where_a_frame_would() {
    let display = Spin { fps: rune_motion::Cycle::DISPLAY_RATE, ..Spin::default() };
    let (mut framed, mut stepped) = (Host::with_poll(Spin { ..display }, None), Host::with_poll(Spin { ..display }, None));
    for host in [&mut framed, &mut stepped] {
        for _ in 0..3 {
            host.frame();
        }
    }
    let (paints, before) = (stepped.rt.paint_count(), turns(&stepped.rt));
    framed.frame();
    let patched = stepped.step().expect("a rotation takes a loop-only step");
    assert_eq!(patched.len(), 1, "only the turning glyph changes");
    assert_eq!(stepped.rt.paint_count(), paints, "the step paints nothing");
    let (step, frame) = (turns(&stepped.rt), turns(&framed.rt));
    assert_eq!((step.len(), frame.len()), (1, 1));
    assert!((step[0] - frame[0]).abs() < 1e-4, "step turned to {}, a frame to {}", step[0], frame[0]);
    assert_ne!(step, before, "the step moved the turn on");
}

struct Host {
    rt: Runtime<Spin>,
    clock: crate::frame::clock::FrameClock,
    start: crate::time::Instant,
    frames: u32,
    tick: u32,
}

impl Host {
    fn new(fps: f32) -> Host {
        Host::with_poll(Spin { fps, ..Spin::default() }, None)
    }

    fn with_poll(spin: Spin, poll: Option<fn(&mut Spin, f64, &mut crate::Tx) -> bool>) -> Host {
        let start = crate::time::now();
        let mut rt = Runtime::new(spin, loader, (40.0, 40.0), Env::new(true, 2.0));
        rt.wall_start = Some(start);
        rt.frame.set_test_clock(crate::ManualClock::new(120.0));
        rt.poll = poll;
        let mut host = Host { rt, clock: crate::frame::clock::FrameClock::new(start), start, frames: 0, tick: 2 };
        host.frame();
        host
    }

    fn next(&mut self) -> crate::time::Instant {
        self.frames += self.tick;
        let secs = f64::from(self.frames) / 120.0 + 0.0013;
        self.rt.frame.test_clock_mut().unwrap().seek(secs);
        self.start + std::time::Duration::from_secs_f64(secs)
    }

    fn frame(&mut self) -> bool {
        let at = self.next();
        let (core, mut app) = self.rt.split_mut();
        self.clock.advance_core(core, &mut app, at, false).0
    }

    fn step(&mut self) -> Result<Vec<u32>, &'static str> {
        let at = self.next();
        crate::frame::loop_frame::step(&mut self.clock, self.rt.core_mut(), at).map(|(patched, _)| patched)
    }
}

#[test]
fn a_rotation_with_a_rate_wakes_at_that_rate_and_keeps_its_speed() {
    let (mut framed, mut stepped) = (Host::new(60.0), Host::new(60.0));
    for host in [&mut framed, &mut stepped] {
        for _ in 0..6 {
            assert!(!host.frame(), "a stepped rotation asks for no display-rate frames");
            let due = host.rt.wake_deadline().expect("it wakes for its next step");
            assert!(due - host.rt.clock <= 1.0 / 60.0 + 1e-9, "next step {} s away", due - host.rt.clock);
        }
    }
    let before = turns(&framed.rt)[0];
    framed.frame();
    let turned = (turns(&framed.rt)[0] - before).rem_euclid(std::f32::consts::TAU);
    let full = std::f32::consts::TAU / 0.9 / 60.0;
    assert!((turned - full).abs() < 1e-3, "a frame of a resting window turns it {turned}, a 60th of a second turns it {full}");

    let patched = stepped.step().expect("its wake, reached frame by frame, takes a loop-only step");
    assert_eq!(patched.len(), 1);
    let (step, frame) = (turns(&stepped.rt)[0], turns(&framed.rt)[0]);
    assert!((step - frame).abs() < 1e-4, "step turned to {step}, a frame to {frame}");
    assert!(stepped.rt.wake_deadline().is_some_and(|next| next > stepped.rt.clock), "the step armed the next wake");
}

#[test]
fn a_spinner_beside_an_idle_poll_runs_the_poll_only_at_its_own_wakes() {
    fn poll(spin: &mut Spin, clock: f64, tx: &mut crate::Tx) -> bool {
        spin.polls += 1;
        while spin.next <= clock {
            spin.next += 0.25;
        }
        tx.wake_at(spin.next);
        false
    }
    for dots in [false, true] {
        let mut host = Host::with_poll(Spin { fps: 60.0, dots, ..Spin::default() }, Some(poll));
        host.rt.state.polls = 0;
        let (mut steps, mut refused) = (0, Vec::new());
        for _ in 0..120 {
            match host.step() {
                Ok(_) => steps += 1,
                Err(why) => {
                    refused.push(why);
                    host.frames -= host.tick;
                    host.frame();
                }
            }
        }
        let (polls, what) = (host.rt.state.polls, if dots { "dots" } else { "a turning arc" });
        assert!(polls <= 9, "two seconds of {what} at 60 fps ran the app's 4-a-second poll {polls} times; refusals: {refused:?}");
        assert!(steps >= 110, "{what}: {steps} of 120 frames were loop-only; refusals: {refused:?}");
    }
}

#[test]
fn a_spinner_scrolled_out_of_sight_leaves_the_one_in_sight_stepping() {
    let spin = || Spin { fps: 60.0, dots: true, scrolled: true, ..Spin::default() };
    let (mut framed, mut stepped) = (Host::with_poll(spin(), None), Host::with_poll(spin(), None));
    for host in [&mut framed, &mut stepped] {
        for _ in 0..3 {
            host.frame();
        }
    }
    framed.frame();
    let patched = stepped.step().expect("the dot out of sight draws nothing, so the one in sight steps alone");
    assert_eq!(patched.len(), 1);
    assert_eq!(bytemuck::cast_slice::<_, u8>(&stepped.rt.instances), bytemuck::cast_slice::<_, u8>(&framed.rt.instances), "the step draws what a frame would");
}

#[test]
fn a_spinner_under_a_faded_row_steps_and_keeps_the_row_s_opacity() {
    let spin = || Spin { fps: 60.0, dots: true, faded: true, ..Spin::default() };
    let (mut framed, mut stepped) = (Host::with_poll(spin(), None), Host::with_poll(spin(), None));
    for host in [&mut framed, &mut stepped] {
        for _ in 0..3 {
            host.frame();
        }
    }
    framed.frame();
    let patched = stepped.step().expect("a dot alone in a faded row and one beside a label in another both step");
    assert_eq!(patched.len(), 2);
    assert_eq!(bytemuck::cast_slice::<_, u8>(&stepped.rt.instances), bytemuck::cast_slice::<_, u8>(&framed.rt.instances), "the step draws what a frame would");
}

#[test]
fn a_late_loop_frame_still_steps_and_lands_where_a_frame_would() {
    for (spin, what) in [(Spin { fps: 60.0, dots: true, ..Spin::default() }, "dots at 60 fps"),
        (Spin { fps: rune_motion::Cycle::DISPLAY_RATE, ..Spin::default() }, "an arc at the display rate")] {
        let make = || Host::with_poll(Spin { ..spin }, None);
        let (mut framed, mut stepped) = (make(), make());
        for host in [&mut framed, &mut stepped] {
            for _ in 0..3 {
                host.frame();
            }
            host.tick = 12;
        }
        framed.frame();
        stepped.step().unwrap_or_else(|why| panic!("{what}: a loop frame 100 ms late steps: {why:?}"));
        assert_eq!(bytemuck::cast_slice::<_, u8>(&stepped.rt.instances), bytemuck::cast_slice::<_, u8>(&framed.rt.instances), "{what}: the late step draws what a frame would");
    }
}

#[test]
fn a_loop_frame_before_its_wake_redraws_what_is_drawn() {
    let mut host = Host::with_poll(Spin { fps: 60.0, dots: true, ..Spin::default() }, None);
    for _ in 0..3 {
        host.frame();
    }
    let drawn = host.rt.instances.clone();
    host.tick = 1;
    assert_eq!(host.step(), Ok(Vec::new()), "half way to its wake, nothing moves and nothing runs");
    assert_eq!(bytemuck::cast_slice::<_, u8>(&host.rt.instances), bytemuck::cast_slice::<_, u8>(&drawn));
    assert_eq!(host.step().map(|p| p.len()), Ok(3), "at its wake it steps");
}
