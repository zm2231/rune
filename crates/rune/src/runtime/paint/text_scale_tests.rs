use crate::Runtime;
use crate::memo::memo_with_hash;
use crate::{Cx, El, Env, Provided, TextStyle, col, el, oklch, row, text};

const STOPS: &[(f32, f32)] = &[(0.0, 1.0), (0.1, 1.0), (0.5, 0.2), (1.0, 0.2)];
const FRAME: f64 = 1.0 / 30.0;

struct App {
    scale: Provided<f32>,
}

fn spinner(k: usize) -> El {
    let mut e = el().role("progressbar").label("Loading").size(16.0, 16.0).pixel_snap(false);
    for d in 0..8 {
        let (x, y) = ((d % 3) as f32 * 5.0, (d / 3) as f32 * 5.0);
        let c = crate::Cycle { period: 1.8, phase: d as f32 / 8.0, stops: STOPS, still: 0.5, fps: 30.0 };
        e = e.child(el().key(format!("s{k}d{d}")).absolute(x, y).size(3.0, 3.0).radius(1.5).fill(oklch(0.6, 0.1, 250.0)).a11y_hidden().cycle(c));
    }
    e
}

fn view(s: &App, cx: &mut Cx<App>) -> El {
    cx.provide(&s.scale, |cx| {
        let k = *cx.provided::<f32>().expect("scale");
        let style = TextStyle::new(13.0 * k, 500.0);
        memo_with_hash("frame", 7, move || {
            let bar = row().key("toolbar").w_full().h((48.0f32).max(32.0 * k)).px(12.0).gap(8.0).center_y().fill(oklch(0.25, 0.0, 0.0))
                .child(text("A session title in the toolbar crumb").key("crumb").font(style).truncate().shrink().min_w(0.0));
            let rows = (0..10).map(|i| {
                let r = row().key(format!("row{i}")).w_full().gap(6.0).px(8.0).py(4.0).center_y();
                let r = if i % 3 == 0 { r.child(spinner(i)) } else { r.child(el().size(16.0, 16.0)) };
                r.child(text(format!("Session {i}: a fairly long session title that truncates")).key(format!("title{i}")).font(style).truncate().shrink().min_w(0.0))
            });
            col().w_full().h_full().fill(oklch(0.2, 0.0, 0.0)).child(bar).child(col().key("sessions").w_full().children(rows))
        })
    })
}

fn window(scale: f32) -> Runtime<App> {
    let mut rt = Runtime::new(App { scale: Provided::new(scale) }, view, (420.0, 420.0), Env::new(true, 2.0));
    rt.wall_start = Some(crate::time::now());
    rt.frame.set_test_clock(crate::ManualClock::new(30.0));
    rt.core_mut().paint.keep.force_copy = true;
    rt.frame(0.0);
    rt.frame(crate::frame::clock::RESUME_STEP);
    rt
}

fn tick(rt: &mut Runtime<App>, frames: usize) -> usize {
    let mut stepped = 0;
    for _ in 0..frames {
        let clock = rt.frame.test_clock_mut().unwrap();
        clock.seek(clock.now() + FRAME);
        if rt.loop_step(FRAME).is_ok() { stepped += 1; } else { rt.frame(FRAME); }
    }
    stepped
}

fn shapes(rt: &Runtime<App>) -> Vec<String> {
    rt.instances.iter().map(|i| format!("{:?} {:?} {:?} {:?} {:?} {:?}", i.rect, i.radii, i.fill, i.params[0..3].to_vec(), i.clip, i.fade)).collect()
}

#[test]
fn a_text_scale_step_between_loop_only_frames_paints_what_a_fresh_window_paints() {
    for path in [&[1.6, 1.0][..], &[1.0, 1.6, 1.0][..]] {
        let mut rt = window(path[0]);
        let mut stepped = tick(&mut rt, 30);
        for &to in &path[1..] {
            rt.update(move |s, _| s.scale = Provided::new(to));
            stepped += tick(&mut rt, 30);
        }
        assert!(stepped > 0, "{path:?}: the spinners stepped on loop-only frames");
        let mut fresh = window(*path.last().unwrap());
        tick(&mut fresh, 30 * path.len());
        assert_eq!(rt.frame.test_clock_mut().unwrap().now(), fresh.frame.test_clock_mut().unwrap().now(), "{path:?}: the same time");
        let (a, b) = (shapes(&rt), shapes(&fresh));
        let first = a.iter().zip(&b).position(|(x, y)| x != y);
        assert!(a.len() == b.len() && first.is_none(), "{path:?}: {} instances against {}, first differing {first:?}: {:?} / {:?}", a.len(), b.len(), first.map(|n| &a[n]), first.map(|n| &b[n]));
    }
}

fn drawn(rt: &mut Runtime<App>, frames: usize, gpu: &crate::render::Gpu, renderer: &mut crate::render::Renderer, target: &crate::render::Offscreen) -> Vec<u8> {
    for _ in 0..frames {
        let clock = rt.frame.test_clock_mut().unwrap();
        clock.seek(clock.now() + FRAME);
        let patched = rt.loop_step(FRAME).ok();
        if patched.is_none() { rt.frame(FRAME); }
        renderer.next_instances(rt.paint_count(), patched);
        renderer.render_runtime_to_offscreen(&gpu.device, &gpu.queue, target, rt);
    }
    target.read(&gpu.device)
}

#[test]
fn a_renderer_kept_across_a_text_scale_step_draws_what_a_fresh_one_draws() {
    use crate::render::{Gpu, OFFSCREEN_FORMAT, Offscreen, Renderer};
    let Some(gpu) = Gpu::test_headless() else { return };
    for path in [&[1.6, 1.0][..], &[1.0, 1.6, 1.0][..]] {
        let target = Offscreen::new(&gpu.device, 840, 840);
        let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
        let mut rt = window(path[0]);
        drawn(&mut rt, 30, &gpu, &mut renderer, &target);
        let mut px = Vec::new();
        for &to in &path[1..] {
            rt.update(move |s, _| s.scale = Provided::new(to));
            px = drawn(&mut rt, 30, &gpu, &mut renderer, &target);
        }
        let fresh_target = Offscreen::new(&gpu.device, 840, 840);
        let mut fresh_renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
        let mut fresh = window(*path.last().unwrap());
        drawn(&mut fresh, 30 * (path.len() - 1), &gpu, &mut fresh_renderer, &fresh_target);
        let want = drawn(&mut fresh, 30, &gpu, &mut fresh_renderer, &fresh_target);
        let diff = px.chunks_exact(4).zip(want.chunks_exact(4)).position(|(a, b)| a != b);
        assert!(diff.is_none(), "{path:?}: first differing pixel at {:?}", diff.map(|n| (n % 840, n / 840)));
    }
}
