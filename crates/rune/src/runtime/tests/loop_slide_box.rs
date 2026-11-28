use crate::render::KIND_RECT;
use crate::{Color, Cx, El, Env, Runtime, el, row};

const W: f32 = 200.0;

fn bar(_: &(), _: &mut Cx<()>) -> El {
    let fill = el().key("fill").h_full().radius(999.0).fill(Color::WHITE).w_frac(0.25)
        .looping(rune_motion::LoopSpec::slide(1.4, [-1.0, 0.0], [4.0, 0.0]));
    let track = row().key("track").w(W).h(6.0).radius(999.0).fill(Color::BLACK).clip_soft(0.0).child(fill).a11y_hidden();
    el().padding(10.0).child(track)
}

struct Host {
    rt: Runtime<()>,
    clock: crate::frame::clock::FrameClock,
    start: crate::time::Instant,
    frames: u32,
}

impl Host {
    fn new() -> Host {
        let start = crate::time::now();
        let mut rt = Runtime::new((), bar, (240.0, 40.0), Env::new(true, 2.0));
        rt.wall_start = Some(start);
        rt.frame.set_test_clock(crate::ManualClock::new(120.0));
        let mut host = Host { rt, clock: crate::frame::clock::FrameClock::new(start), start, frames: 0 };
        for _ in 0..3 {
            host.frame();
        }
        host
    }

    fn next(&mut self) -> crate::time::Instant {
        self.frames += 2;
        let secs = f64::from(self.frames) / 120.0 + 0.0013;
        self.rt.frame.test_clock_mut().unwrap().seek(secs);
        self.start + std::time::Duration::from_secs_f64(secs)
    }

    fn frame(&mut self) {
        let at = self.next();
        let (core, mut app) = self.rt.split_mut();
        self.clock.advance_core(core, &mut app, at, false);
    }

    fn step(&mut self) -> Result<Vec<u32>, &'static str> {
        let at = self.next();
        crate::frame::loop_frame::step(&mut self.clock, self.rt.core_mut(), None, at).map(|(patched, _)| patched)
    }

    fn rects(&self) -> Vec<[[f32; 4]; 5]> {
        self.rt.instances.iter().filter(|q| q.params[1] == KIND_RECT && q.rect[0] < q.clip[2] && q.rect[0] + q.rect[2] > q.clip[0]).map(|q| [q.rect, q.radii, q.fill, q.clip, q.params]).collect()
    }
}

#[test]
fn a_sliding_progress_segment_steps_without_a_paint_and_lands_where_a_frame_would() {
    let (mut framed, mut stepped) = (Host::new(), Host::new());
    let mut steps = 0;
    for tick in 0..84 {
        let paints = stepped.rt.paint_count();
        framed.frame();
        match stepped.step() {
            Ok(patched) => {
                steps += 1;
                assert_eq!(patched.len(), 1, "tick {tick}: only the segment's instance moves");
                assert_eq!(stepped.rt.paint_count(), paints, "tick {tick}: the step paints nothing");
            }
            Err(why) => {
                assert!(why == "a sliding box shows or hides" || why == "a sliding box out of sight", "tick {tick}: refused: {why}");
                stepped.frames -= 2;
                stepped.frame();
            }
        }
        let (a, b) = (stepped.rects(), framed.rects());
        assert_eq!(a.len(), b.len(), "tick {tick}: instance count");
        for (p, q) in a.iter().zip(&b) {
            let d = p.iter().flatten().zip(q.iter().flatten()).map(|(x, y)| (x - y).abs()).fold(0.0, f32::max);
            assert!(d < 1e-3, "tick {tick}: the step drew {p:?} where a frame drew {q:?}");
        }
    }
    assert!(steps >= 60, "a segment inside its track steps: {steps} of 84 ticks");
}
