use crate::render::KIND_RECT;
use crate::{Color, Cx, CycleDot, El, Env, Runtime, el};

const STOPS: [&[(f32, f32)]; 3] = [&[(0.0, 0.2), (0.4, 1.0), (1.0, 0.2)], &[(0.0, 1.0), (0.5, 0.3), (1.0, 1.0)], &[(0.0, 0.64)]];

fn dots() -> &'static [CycleDot] {
    static DOTS: std::sync::OnceLock<Vec<CycleDot>> = std::sync::OnceLock::new();
    DOTS.get_or_init(|| {
        (0..9).map(|k| {
            let (x, y) = ((k % 3) as f32 * 0.375, (k / 3) as f32 * 0.375);
            let cycle = crate::Cycle { period: 1.8, phase: k as f32 / 9.0, stops: STOPS[k % 3], still: 0.2 + 0.05 * k as f32, fps: 60.0 };
            CycleDot { rect: [x + 0.0625, y + 0.0625, 0.125, 0.125], radius: 0.24, cycle }
        }).collect()
    })
}

#[derive(Default)]
struct Spin {
    children: bool,
    reduced: bool,
}

const SIDE: f32 = 14.0;

fn spinner(s: &Spin, cx: &mut Cx<Spin>) -> El {
    cx.reduced_motion(s.reduced);
    let ink = Color::WHITE.alpha(0.9);
    let e = el().key("spin").absolute(3.3, 5.7).size(SIDE, SIDE).pixel_snap(false);
    let e = if !s.children { e.fill(ink).cycle_dots(dots()) } else {
        dots().iter().enumerate().fold(e, |e, (k, d)| {
            let side = d.rect[2] * SIDE;
            e.child(el().key(format!("d{k}")).absolute(d.rect[0] * SIDE, d.rect[1] * SIDE).size(side, side).radius(side * d.radius).fill(ink).a11y_hidden().cycle(d.cycle))
        })
    };
    el().size(40.0, 40.0).child(e)
}

struct Host {
    rt: Runtime<Spin>,
    clock: crate::frame::clock::FrameClock,
    start: crate::time::Instant,
    frames: u32,
}

impl Host {
    fn new(spin: Spin) -> Host {
        let start = crate::time::now();
        let mut rt = Runtime::new(spin, spinner, (40.0, 40.0), Env::new(true, 2.0));
        rt.wall_start = Some(start);
        rt.frame.set_test_clock(crate::ManualClock::new(120.0));
        let mut host = Host { rt, clock: crate::frame::clock::FrameClock::new(start), start, frames: 0 };
        host.frame();
        host
    }

    fn next(&mut self) -> crate::time::Instant {
        self.frames += 2;
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
        crate::frame::loop_frame::step(&mut self.clock, self.rt.core_mut(), None, at).map(|(patched, _)| patched)
    }

    fn dots(&self) -> Vec<[[f32; 4]; 4]> {
        self.rt.instances.iter().filter(|q| q.params[1] == KIND_RECT).map(|q| [q.rect, q.radii, q.fill, q.params]).collect()
    }
}

fn assert_same(a: &[[[f32; 4]; 4]], b: &[[[f32; 4]; 4]], what: &str) {
    assert_eq!(a.len(), b.len(), "{what}: instance count");
    for (k, (p, q)) in a.iter().zip(b).enumerate() {
        let d = p.iter().flatten().zip(q.iter().flatten()).map(|(x, y)| (x - y).abs()).fold(0.0, f32::max);
        assert!(d < 1e-4, "{what}: dot {k} differs by {d}: {p:?} against {q:?}");
    }
}

#[test]
fn a_dots_loop_paints_what_a_child_per_dot_paints() {
    for reduced in [false, true] {
        let mut one = Host::new(Spin { children: false, reduced });
        let mut many = Host::new(Spin { children: true, reduced });
        for tick in 0..12 {
            one.frame();
            many.frame();
            assert_eq!(one.dots().len(), dots().len());
            assert_same(&one.dots(), &many.dots(), &format!("reduced {reduced}, frame {tick}"));
        }
        assert_eq!(one.rt.tree.nodes.len() + dots().len(), many.rt.tree.nodes.len());
    }
}

#[test]
fn a_dots_loop_steps_without_a_paint_and_lands_where_a_frame_would() {
    let (mut framed, mut stepped) = (Host::new(Spin::default()), Host::new(Spin::default()));
    for host in [&mut framed, &mut stepped] {
        for _ in 0..3 {
            host.frame();
        }
    }
    for tick in 0..20 {
        let paints = stepped.rt.paint_count();
        framed.frame();
        let patched = stepped.step().expect("a dots loop takes a loop-only step");
        assert_eq!(patched.len(), dots().len(), "each dot is its own slot");
        assert_eq!(stepped.rt.paint_count(), paints, "the step paints nothing");
        assert_same(&stepped.dots(), &framed.dots(), &format!("step {tick}"));
    }
}

#[test]
fn a_dots_loop_wakes_at_its_cycles_rate_and_rests_under_reduced_motion() {
    let host = Host::new(Spin::default());
    assert_eq!(host.rt.motion.loops_len(), 1, "one loop for every dot");
    assert!(host.rt.wake_deadline().is_some(), "a stepped cycle sleeps until its next step");
    let host = Host::new(Spin { reduced: true, ..Spin::default() });
    assert_eq!(host.rt.wake_deadline(), None, "reduced motion holds each dot still");
}

#[test]
fn an_ambient_period_paces_a_loop_until_input_steers_it() {
    let mut host = Host::new(Spin::default());
    host.rt.core_mut().set_ambient_period(Some(std::time::Duration::from_secs_f64(1.0 / 30.0)));
    let interval = |host: &Host| host.rt.core().motion_wake_deadline().unwrap() - host.rt.clock();
    host.frame();
    assert!((interval(&host) - 1.0 / 30.0).abs() < 1e-6, "an unsteered loop wakes at the ambient rate: {}", interval(&host));
    host.rt.core_mut().note_steering_input();
    host.frame();
    assert!((interval(&host) - 1.0 / 60.0).abs() < 1e-6, "input lifts the cap: {}", interval(&host));
    assert!(host.step().is_ok());
    assert!((interval(&host) - 1.0 / 60.0).abs() < 1e-6, "a step while steered keeps the loop's rate: {}", interval(&host));
    for _ in 0..40 {
        assert!(host.step().is_ok(), "only the loop is due");
    }
    assert!((interval(&host) - 1.0 / 30.0).abs() < 1e-6, "steps past the steered window pace again: {}", interval(&host));
}
