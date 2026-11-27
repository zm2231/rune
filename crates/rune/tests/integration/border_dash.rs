use rune::render::{Gpu, KIND_DASHED, KIND_RECT, OFFSCREEN_FORMAT_LINEAR, Offscreen, Renderer};
use rune::{Color, Cx, Dash, El, Env, LineCap, Runtime, Shape, canvas, el};

const TOP: Color = Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
const BOTTOM: Color = Color { r: 0.0, g: 0.0, b: 1.0, a: 1.0 };
const RIM: Color = Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
const DARK: Color = Color { r: 0.1, g: 0.1, b: 0.12, a: 1.0 };

type Build = fn() -> El;

fn frame(build: Build, size: (f32, f32)) -> Runtime<Build> {
    let mut rt = Runtime::new(build, |b, _: &mut Cx<Build>| el().size(1000.0, 1000.0).child(b()), size, Env::new(true, 2.0));
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
}

fn pixels(gpu: &Gpu, build: Build, size: (f32, f32)) -> Image {
    let mut rt = frame(build, size);
    let px = ((size.0 * 2.0) as u32, (size.1 * 2.0) as u32);
    let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT_LINEAR);
    let target = Offscreen::with_format(&gpu.device, px.0, px.1, OFFSCREEN_FORMAT_LINEAR);
    renderer.prepare(&gpu.device, &gpu.queue, &mut rt.text.atlas, &rt.instances, px);
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    renderer.draw(&mut encoder, &target.view, [0.0; 4]);
    target.copy(&mut encoder);
    gpu.queue.submit([encoder.finish()]);
    Image { rgba: target.read(&gpu.device), width: px.0 as usize }
}

fn capsule() -> El {
    el().absolute(4.0, 4.0).size(60.0, 14.0).radius(7.0)
}

const CAPSULE_WINDOW: (f32, f32) = (68.0, 22.0);

#[test]
fn a_dashed_border_is_its_own_ring_over_the_fill() {
    let rt = frame(|| capsule().fill(DARK).border_dash(8.0, 4.0).border_dash_phase(2.0).border(1.0, RIM), CAPSULE_WINDOW);
    let [fill, ring] = rt.instances.as_slice() else { panic!("fill and ring: {:?}", rt.instances) };
    assert_eq!((fill.params[0], fill.params[1], fill.fill), (0.0, KIND_RECT, DARK.premul(1.0)), "the fill has no ring");
    assert_eq!((ring.params[0], ring.params[1]), (2.0, KIND_DASHED), "1 pt at 2x");
    assert_eq!((ring.rect, ring.radii), ([8.0, 8.0, 120.0, 28.0], [14.0; 4]));
    assert_eq!((ring.fill, ring.border), (RIM.premul(1.0), RIM.premul(1.0)));
    assert_eq!(ring.uv, [16.0, 8.0, 4.0, 0.0], "dash, gap, phase in device pixels, butt ends");

    let rt = frame(|| capsule().border_gradient(1.0, TOP, BOTTOM).border_dash(8.0, 4.0).border_dash_cap(LineCap::Round), CAPSULE_WINDOW);
    let [ring] = rt.instances.as_slice() else { panic!("only the ring: {:?}", rt.instances) };
    assert_eq!((ring.params[1], ring.fill, ring.border), (KIND_DASHED, TOP.premul(1.0), BOTTOM.premul(1.0)));
    assert_eq!(ring.uv[3], 1.0, "round ends");

    let rt = frame(|| capsule().border(1.0, RIM).border_dash(0.0, 4.0), CAPSULE_WINDOW);
    let [ring] = rt.instances.as_slice() else { panic!("one ring: {:?}", rt.instances) };
    assert_eq!((ring.params[0], ring.params[1]), (2.0, KIND_RECT), "a zero dash with butt ends draws the border solid");
}

#[test]
fn dashes_centre_on_corners_and_antialias_like_the_solid_border() {
    let Some(gpu) = Gpu::test_headless() else { return };
    let solid = pixels(&gpu, || capsule().border(1.0, RIM), CAPSULE_WINDOW);
    let dashed = pixels(&gpu, || capsule().border(1.0, RIM).border_dash(8.0, 4.0), CAPSULE_WINDOW);
    for y in 6..12 {
        assert_eq!(dashed.px(32, y), solid.px(32, y), "row {y}: the dash 24 px from the top-left corner's middle spans x = 27.8..43.8");
        assert_eq!(dashed.px(47, y)[3], 0, "row {y}: the gap after it spans x = 43.8..51.8");
    }
    assert!(solid.px(32, 8)[3] > 250 && solid.px(32, 9)[3] > 250);
    let corner: Vec<u8> = (0..=16)
        .map(|n| {
            let a = n as f32 / 16.0 * std::f32::consts::FRAC_PI_2;
            dashed.px((114.0 + 13.0 * a.sin()) as usize, (22.0 - 13.0 * a.cos()) as usize)[3]
        })
        .collect();
    assert!(
        corner[0] == 0 && corner[4..13].iter().all(|a| *a > 150) && corner[16] == 0,
        "round the top-right corner's centre line (radius 13 about (114, 22)) a dash is centred on 45 degrees, 35 degrees each way: {corner:?}"
    );

    let filled = pixels(&gpu, || capsule().fill(DARK).border(1.0, RIM).border_dash(8.0, 4.0), CAPSULE_WINDOW);
    let dark = filled.px(68, 20);
    assert!(dark[3] > 250 && dark[0] < 100, "inside is the fill: {dark:?}");
    assert_eq!(filled.px(47, 8), dark, "the fill shows through a gap");
    assert!(filled.px(32, 8)[0] > 250, "a dash draws over the fill");
}

fn box_wide(w: f32) -> El {
    el().absolute(4.0, 4.0).size(w, 20.0).radius(6.0).border(1.0, RIM).border_dash(6.0, 4.0)
}

#[test]
fn dashes_hold_their_places_while_a_box_grows() {
    let Some(gpu) = Gpu::test_headless() else { return };
    let narrow = pixels(&gpu, || box_wide(50.0), (72.0, 28.0));
    let wide = pixels(&gpu, || box_wide(56.0), (72.0, 28.0));
    for y in 0..56 {
        for x in 0..56 {
            assert_eq!(wide.px(x, y), narrow.px(x, y), "({x}, {y}): left of the middle (x = 58 px, then 64) nothing moves");
        }
        for x in 60..116 {
            assert_eq!(wide.px(x + 12, y), narrow.px(x, y), "({x}, {y}): right of it the dashes move with the right edge");
        }
    }
    assert!((0..56).any(|y| narrow.px(30, y)[3] > 250), "there are dashes to compare");
}

fn square(dash: f32, gap: f32, cap: LineCap) -> El {
    el().absolute(4.0, 4.0).size(40.0, 20.0).border(2.0, RIM).border_dash(dash, gap).border_dash_cap(cap)
}

#[test]
fn round_caps_reach_half_a_border_past_each_dash_and_a_zero_dash_draws_dots() {
    let Some(gpu) = Gpu::test_headless() else { return };
    let size = (48.0, 28.0);
    let butt = pixels(&gpu, || square(8.0, 8.0, LineCap::Butt), size);
    let round = pixels(&gpu, || square(8.0, 8.0, LineCap::Round), size);
    assert!(butt.px(16, 9)[3] > 250 && round.px(16, 9)[3] > 250, "both draw the dash centred on the corner, to x = 18");
    assert_eq!(butt.px(19, 9)[3], 0, "a butt dash stops at its length");
    assert!(round.px(19, 9)[3] > 200, "a round one goes on: {:?}", round.px(19, 9));
    assert_eq!(round.px(21, 9)[3], 0, "for half the border's width, 2 px");
    let dots = pixels(&gpu, || square(0.0, 6.0, LineCap::Round), size);
    for (x, y) in [(9, 9), (10, 10), (21, 9), (22, 10)] {
        assert!(dots.px(x, y)[3] > 200, "dots on the corner (10, 10) and every 12 px: ({x}, {y}) {:?}", dots.px(x, y));
    }
    assert_eq!(dots.px(16, 9)[3], 0, "between dots");
    assert_eq!(dots.px(22, 13)[3], 0, "a dot is round, not a dash across the ring");
}

#[test]
fn a_dashed_stroke_draws_its_dashes() {
    let Some(gpu) = Gpu::test_headless() else { return };
    let line = || {
        canvas(Shape::new().stroke_dashed(vec![[0.0, 2.0], [40.0, 2.0]], false, 2.0, Dash::new(4.0, 4.0), RIM))
            .absolute(4.0, 4.0)
            .size(40.0, 4.0)
    };
    let img = pixels(&gpu, line, (48.0, 12.0));
    for x in [9, 15, 25, 31] {
        assert!(img.px(x, 11)[3] > 250, "dashes at x = 8..16, 24..32: x = {x} {:?}", img.px(x, 11));
    }
    for x in [17, 20, 23] {
        assert_eq!(img.px(x, 11)[3], 0, "gap at x = {x}");
    }
}
