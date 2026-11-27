use rune::render::{Gpu, KIND_DASHED, KIND_RECT, OFFSCREEN_FORMAT_LINEAR, Offscreen, Renderer};
use rune::{Color, Cx, Dash, El, Env, LineCap, Runtime, Shape, canvas, el};

const TOP: Color = Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
const BOTTOM: Color = Color { r: 0.0, g: 0.0, b: 1.0, a: 1.0 };
const RIM: Color = Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
const DARK: Color = Color { r: 0.1, g: 0.1, b: 0.12, a: 1.0 };

type Build = fn() -> El;

fn frame(build: Build, size: (f32, f32), scale: f32) -> Runtime<Build> {
    let mut rt = Runtime::new(build, |b, _: &mut Cx<Build>| el().size(1000.0, 1000.0).child(b()), size, Env::new(true, scale));
    rt.set_srgb_blend(false);
    rt.frame(0.0);
    rt
}

struct Image {
    rgba: Vec<u8>,
    width: usize,
}

impl Image {
    fn px(&self, x: usize, y: usize) -> [u8; 4] {
        self.rgba[(y * self.width + x) * 4..][..4].try_into().unwrap()
    }

    fn alpha(&self, x: usize, y: usize) -> u8 {
        self.px(x, y)[3]
    }

    fn centres(&self, y: usize, xs: std::ops::Range<usize>) -> Vec<f32> {
        let mut out = Vec::new();
        let (mut sum, mut weight) = (0.0f32, 0.0f32);
        for x in xs {
            let a = self.alpha(x, y) as f32;
            if a > 0.0 {
                sum += a * (x as f32 + 0.5);
                weight += a;
            } else if weight > 0.0 {
                out.push(sum / weight);
                (sum, weight) = (0.0, 0.0);
            }
        }
        out
    }

    fn patch(&self, cx: usize, cy: usize) -> Vec<u8> {
        (cy - 2..=cy + 2).flat_map(|y| (cx - 2..=cx + 2).map(move |x| (x, y))).map(|(x, y)| self.alpha(x, y)).collect()
    }
}

fn draw<S>(gpu: &Gpu, rt: &mut Runtime<S>, px: (u32, u32)) -> Image {
    let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT_LINEAR);
    let target = Offscreen::with_format(&gpu.device, px.0, px.1, OFFSCREEN_FORMAT_LINEAR);
    renderer.prepare(&gpu.device, &gpu.queue, &mut rt.text.atlas, &rt.instances, px);
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    renderer.draw(&mut encoder, &target.view, [0.0; 4]);
    target.copy(&mut encoder);
    gpu.queue.submit([encoder.finish()]);
    Image { rgba: target.read(&gpu.device), width: px.0 as usize }
}

fn pixels(gpu: &Gpu, build: Build, size: (f32, f32), scale: f32) -> Image {
    let mut rt = frame(build, size, scale);
    draw(gpu, &mut rt, ((size.0 * scale) as u32, (size.1 * scale) as u32))
}

fn capsule() -> El {
    el().absolute(4.0, 4.0).size(60.0, 14.0).radius(7.0)
}

const CAPSULE_WINDOW: (f32, f32) = (68.0, 22.0);

fn counts(packed: f32) -> [f32; 2] {
    [(packed / 4096.0).floor(), packed % 4096.0]
}

#[test]
fn a_dashed_border_is_its_own_ring_over_the_fill() {
    let rt = frame(|| capsule().fill(DARK).border_dash(8.0, 4.0).border_dash_phase(2.0).border(1.0, RIM), CAPSULE_WINDOW, 2.0);
    let [fill, ring] = rt.instances.as_slice() else { panic!("fill and ring: {:?}", rt.instances) };
    assert_eq!((fill.params[0], fill.params[1], fill.fill), (0.0, KIND_RECT, DARK.premul(1.0)), "the fill has no ring");
    assert_eq!((ring.params[0], ring.params[1]), (2.0, KIND_DASHED), "1 pt at 2x");
    assert_eq!((ring.rect, ring.radii), ([8.0, 8.0, 120.0, 28.0], [14.0; 4]));
    assert_eq!((ring.fill, ring.border), (RIM.premul(1.0), RIM.premul(1.0)));
    assert_eq!(&ring.uv[..3], &[16.0, 8.0, 4.0], "dash, gap, phase in device pixels, butt ends");
    let quarter = 13.0 * std::f32::consts::FRAC_PI_2;
    assert_eq!(
        (counts(ring.params[3]), counts(ring.uv[3])),
        ([5.0, 1.0], [5.0, 1.0]),
        "the long sides ({} px) hold 5 periods of 24, the ends ({quarter} px, all corner) 1",
        92.0 + quarter
    );

    let rt = frame(|| capsule().border_gradient(1.0, TOP, BOTTOM).border_dash(8.0, 4.0).border_dash_cap(LineCap::Round), CAPSULE_WINDOW, 2.0);
    let [ring] = rt.instances.as_slice() else { panic!("only the ring: {:?}", rt.instances) };
    assert_eq!((ring.params[1], ring.fill, ring.border), (KIND_DASHED, TOP.premul(1.0), BOTTOM.premul(1.0)));
    assert_eq!(ring.uv[1], -8.0, "round ends turn the gap negative");

    let rt = frame(|| capsule().border(1.0, RIM).border_dash(0.0, 4.0), CAPSULE_WINDOW, 2.0);
    let [ring] = rt.instances.as_slice() else { panic!("one ring: {:?}", rt.instances) };
    assert_eq!((ring.params[0], ring.params[1]), (2.0, KIND_RECT), "a zero dash with butt ends draws the border solid");
}

#[test]
fn dashes_centre_on_corners_and_antialias_like_the_solid_border() {
    let Some(gpu) = Gpu::test_headless() else { return };
    let solid = pixels(&gpu, || capsule().border(1.0, RIM), CAPSULE_WINDOW, 2.0);
    let dashed = pixels(&gpu, || capsule().border(1.0, RIM).border_dash(8.0, 4.0), CAPSULE_WINDOW, 2.0);
    let (mut inside, mut gaps) = (0, 0);
    for x in 24..106 {
        let column = |img: &Image| (6..12).map(|y| img.px(x, y)).collect::<Vec<_>>();
        let lit = |x: usize| dashed.alpha(x, 8) > 0;
        if lit(x - 1) && lit(x) && lit(x + 1) {
            assert_eq!(column(&dashed), column(&solid), "x = {x}: inside a dash the rows match the solid border");
            inside += 1;
        } else if !lit(x) {
            assert!((6..12).all(|y| dashed.alpha(x, y) == 0), "x = {x}: a gap is empty");
            gaps += 1;
        }
    }
    assert!(inside > 40 && gaps > 15, "the top run has dashes and gaps: {inside} {gaps}");
    assert!(solid.alpha(32, 8) > 250 && solid.alpha(32, 9) > 250);
    let corner: Vec<u8> = (0..=16)
        .map(|n| {
            let a = n as f32 / 16.0 * std::f32::consts::FRAC_PI_2;
            dashed.alpha((114.0 + 13.0 * a.sin()) as usize, (22.0 - 13.0 * a.cos()) as usize)
        })
        .collect();
    assert!(
        corner[0] == 0 && corner[4..13].iter().all(|a| *a > 150) && corner[16] == 0,
        "round the top-right corner's centre line (radius 13 about (114, 22)) a dash is centred on 45 degrees, 35 degrees each way: {corner:?}"
    );

    let filled = pixels(&gpu, || capsule().fill(DARK).border(1.0, RIM).border_dash(8.0, 4.0), CAPSULE_WINDOW, 2.0);
    let dark = filled.px(68, 20);
    assert!(dark[3] > 250 && dark[0] < 100, "inside is the fill: {dark:?}");
    let gap = (24..106).find(|&x| dashed.alpha(x, 8) == 0).unwrap();
    assert_eq!(filled.px(gap, 8), dark, "the fill shows through a gap");
    let dash = (24..106).find(|&x| dashed.alpha(x, 8) > 250).unwrap();
    assert!(filled.px(dash, 8)[0] > 250, "a dash draws over the fill");
}

fn dotted() -> El {
    el().absolute(8.0, 8.0).size(96.0, 64.0).radius(16.0).border(2.0, RIM).border_dash(0.0, 5.0).border_dash_cap(LineCap::Round)
}

fn dashed_notice() -> El {
    el().absolute(8.0, 8.0).size(150.0, 64.0).radius(10.0).border(1.0, RIM).border_dash(4.0, 3.0)
}

#[test]
fn every_gap_along_a_side_matches_the_middle_one_included() {
    let Some(gpu) = Gpu::test_headless() else { return };
    for (build, w, bw, scale) in [(dotted as Build, 96.0, 2.0, 1.5), (dotted, 96.0, 2.0, 2.0), (dashed_notice, 150.0, 1.0, 1.0), (dashed_notice, 150.0, 1.0, 1.5)] {
        let img = pixels(&gpu, build, (w + 16.0, 80.0), scale);
        let ring_px = ((bw * scale) as usize).max(1);
        let row = (8.0 * scale) as usize + (ring_px - 1) / 2;
        let left = ((8.0 + 18.0) * scale) as usize;
        let right = ((8.0 + w - 18.0) * scale) as usize;
        let centres = img.centres(row, left..right);
        let centres = &centres[1..centres.len() - 1];
        let steps: Vec<f32> = centres.windows(2).map(|c| c[1] - c[0]).collect();
        let (lo, hi) = steps.iter().fold((f32::MAX, 0.0f32), |(lo, hi), s| (lo.min(*s), hi.max(*s)));
        assert!(steps.len() >= 6, "{w} at {scale}x: enough dashes along the top: {centres:?}");
        assert!(hi - lo <= 1.05, "{w} at {scale}x: dash spacing varies by at most a pixel, the middle too: {steps:?}");
    }
}

#[test]
fn round_dots_are_round_and_alike_at_fractional_scales() {
    let Some(gpu) = Gpu::test_headless() else { return };
    let img = pixels(&gpu, dotted, (112.0, 80.0), 1.5);
    let (row, left, right) = (13, 40, 132);
    let peaks: Vec<usize> = (left..right).filter(|&x| img.alpha(x, row - 1) > 200 && img.alpha(x - 1, row - 1) < 200 && img.alpha(x + 1, row - 1) < 200).collect();
    assert!(peaks.len() >= 8, "a dot on every step along the top: {peaks:?}");
    let first = img.patch(peaks[0], row);
    for &x in &peaks {
        assert_eq!(img.patch(x, row), first, "the dot at x = {x} draws as the one at {}", peaks[0]);
    }
    let [up, down, left_of, right_of] = [img.alpha(peaks[0], row - 1), img.alpha(peaks[0], row + 1), img.alpha(peaks[0] - 1, row), img.alpha(peaks[0] + 1, row)];
    let diagonals = [img.alpha(peaks[0] - 1, row - 1), img.alpha(peaks[0] + 1, row - 1), img.alpha(peaks[0] - 1, row + 1), img.alpha(peaks[0] + 1, row + 1)];
    assert!([up, down, left_of, right_of].iter().all(|a| *a > 200), "a 3 px dot centred on a pixel fills its four neighbours: {first:?}");
    assert!(diagonals.iter().all(|a| *a < 170), "and only part of its corners, as a disc does: {first:?}");
    assert!([peaks[0] - 2, peaks[0] + 2].iter().all(|&x| img.alpha(x, row) == 0), "3 px across: {first:?}");
}

fn growing(w: &f32) -> El {
    el().absolute(4.0, 4.0).size(*w, 20.0).radius(6.0).border(1.0, RIM).border_dash(6.0, 4.0)
}

fn sized(w: f32) -> Runtime<f32> {
    let mut rt = Runtime::new(w, |w: &f32, _: &mut Cx<f32>| el().size(1000.0, 1000.0).child(growing(w)), (96.0, 28.0), Env::new(true, 2.0));
    rt.set_srgb_blend(false);
    rt.frame(0.0);
    rt
}

fn resize(rt: &mut Runtime<f32>, w: f32, t: f64) {
    rt.update(|s, tx| {
        tx.instant();
        *s = w;
    });
    rt.frame(t);
}

fn top_count(rt: &Runtime<f32>) -> f32 {
    let ring = rt.instances.iter().find(|i| i.params[1] == KIND_DASHED).expect("a dashed ring");
    counts(ring.params[3])[0]
}

#[test]
fn a_growing_box_keeps_its_corners_and_moves_its_dashes_smoothly_but_for_one_step_per_count() {
    let Some(gpu) = Gpu::test_headless() else { return };
    let mut rt = sized(50.0);
    let px = (192, 56);
    let first = draw(&gpu, &mut rt, px);
    let mut before = first.centres(9, 30..191);
    let mut count = top_count(&rt);
    let (mut changes, mut jumps) = (0, 0);
    for k in 1..=160 {
        let w = 50.0 + k as f32 * 0.25;
        resize(&mut rt, w, k as f64 / 60.0);
        let img = draw(&gpu, &mut rt, px);
        for y in 0..56 {
            for x in 0..22 {
                assert_eq!(img.px(x, y), first.px(x, y), "w = {w}: ({x}, {y}) by the left corners never changes");
            }
        }
        let now = top_count(&rt);
        assert!(now == count || now == count + 1.0, "w = {w}: the count only grows, one at a time: {count} then {now}");
        let centres = img.centres(9, 30..191);
        let moved = centres.iter().zip(&before).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        if now != count {
            changes += 1;
        }
        if moved > 2.0 || centres.len() != before.len() {
            jumps += 1;
        }
        (before, count) = (centres, now);
    }
    assert_eq!(changes, 3, "growing 40 pt adds three dashes to the top");
    assert_eq!(jumps, changes, "dashes only jump when the count changes, and move under two pixels otherwise");
}

#[test]
fn a_box_settling_by_a_count_boundary_keeps_its_count() {
    let boundary = (6.5 * 20.0 + 6.72) / 2.0;
    let fresh = |w: f32| top_count(&sized(w));
    assert_eq!((fresh(boundary - 0.5), fresh(boundary + 0.5)), (6.0, 7.0), "either side of {boundary} pt a box starts with 6 or 7");
    let mut rt = sized(boundary - 0.5);
    for k in 0..12 {
        let w = boundary + if k % 2 == 0 { 0.5 } else { -0.5 };
        resize(&mut rt, w, k as f64 / 60.0);
        assert_eq!(top_count(&rt), 6.0, "w = {w}: a box wobbling across the boundary keeps 6");
    }
    resize(&mut rt, boundary + 3.0, 1.0);
    assert_eq!(top_count(&rt), 7.0, "a quarter period past it, it takes 7");
}

fn gapped(gap: &f32, _: &mut Cx<f32>) -> El {
    el().size(1000.0, 1000.0).child(el().absolute(4.0, 4.0).size(50.0, 20.0).radius(6.0).border(1.0, RIM).border_dash(6.0, *gap))
}

#[test]
fn a_new_dash_pattern_takes_its_own_count_rather_than_the_held_one() {
    let start = |gap: f32| {
        let mut rt = Runtime::new(gap, gapped, (96.0, 28.0), Env::new(true, 2.0));
        rt.frame(0.0);
        rt
    };
    let held = |rt: &Runtime<f32>| rt.instances.iter().find(|i| i.params[1] == KIND_DASHED).map(|i| (i.params[3], i.uv[3])).unwrap();
    let fresh = start(4.4);
    let mut rt = start(4.0);
    assert_ne!(held(&rt), held(&fresh), "the two patterns fit different counts on a 50 pt box");
    rt.update(|g, tx| {
        tx.instant();
        *g = 4.4;
    });
    rt.frame(0.1);
    assert_eq!(held(&rt), held(&fresh), "after the change the box counts as a fresh one with the new pattern");
}

fn square(dash: f32, gap: f32, cap: LineCap) -> El {
    el().absolute(4.0, 4.0).size(40.0, 20.0).border(2.0, RIM).border_dash(dash, gap).border_dash_cap(cap)
}

#[test]
fn round_caps_reach_half_a_border_past_each_dash_and_a_zero_dash_draws_dots() {
    let Some(gpu) = Gpu::test_headless() else { return };
    let size = (48.0, 28.0);
    let butt = pixels(&gpu, || square(8.0, 8.0, LineCap::Butt), size, 2.0);
    let round = pixels(&gpu, || square(8.0, 8.0, LineCap::Round), size, 2.0);
    assert!(butt.alpha(16, 9) > 250 && round.alpha(16, 9) > 250, "both draw the dash centred on the corner, to x = 18");
    assert_eq!(butt.alpha(19, 9), 0, "a butt dash stops at its length");
    assert!(round.alpha(19, 9) > 200, "a round one goes on: {:?}", round.px(19, 9));
    assert_eq!(round.alpha(21, 9), 0, "for half the border's width, 2 px");
    let dots = pixels(&gpu, || square(0.0, 6.0, LineCap::Round), size, 2.0);
    for (x, y) in [(9, 9), (10, 10), (21, 9), (22, 10)] {
        assert!(dots.alpha(x, y) > 200, "dots on the corner (10, 10) and every 12 px: ({x}, {y}) {:?}", dots.px(x, y));
    }
    assert_eq!(dots.alpha(16, 9), 0, "between dots");
    assert_eq!(dots.alpha(22, 13), 0, "a dot is round, not a dash across the ring");
}

#[test]
fn a_dashed_stroke_draws_its_dashes() {
    let Some(gpu) = Gpu::test_headless() else { return };
    let line = || {
        canvas(Shape::new().stroke_dashed(vec![[0.0, 2.0], [40.0, 2.0]], false, 2.0, Dash::new(4.0, 4.0), RIM))
            .absolute(4.0, 4.0)
            .size(40.0, 4.0)
    };
    let img = pixels(&gpu, line, (48.0, 12.0), 2.0);
    for x in [9, 15, 25, 31] {
        assert!(img.alpha(x, 11) > 250, "dashes at x = 8..16, 24..32: x = {x} {:?}", img.px(x, 11));
    }
    for x in [17, 20, 23] {
        assert_eq!(img.alpha(x, 11), 0, "gap at x = {x}");
    }
}
