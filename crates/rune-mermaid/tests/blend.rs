use rune::render::{Gpu, OFFSCREEN_FORMAT, OFFSCREEN_FORMAT_LINEAR, Offscreen, Renderer};
use rune::{Color, Cx, El, Env, Runtime, el, row};
use rune_mermaid::DiagramStyle;

const W: u32 = 80;
const H: u32 = 40;

#[derive(Clone, Copy)]
struct Pair {
    surface: Color,
    top: Color,
    flat: Color,
}

fn view(p: &Pair, _: &mut Cx<Pair>) -> El {
    let half = W as f32 / 2.0;
    row().size(W as f32, H as f32).fill(p.surface).child(el().size(half, H as f32).fill(p.top)).child(el().size(half, H as f32).fill(p.flat))
}

fn drawn(gpu: &Gpu, pair: Pair, srgb: bool) -> ([u8; 3], [u8; 3]) {
    let mut rt = Runtime::new(pair, view, (W as f32, H as f32), Env::new(false, 1.0));
    rt.set_srgb_blend(srgb);
    rt.frame(0.0);
    rt.settle_all();
    rt.frame(0.0);
    let format = if srgb { OFFSCREEN_FORMAT } else { OFFSCREEN_FORMAT_LINEAR };
    let mut renderer = Renderer::new(&gpu.device, format);
    renderer.set_srgb_blend(srgb);
    let target = Offscreen::with_format(&gpu.device, W, H, format);
    renderer.render_runtime_to_offscreen(&gpu.device, &gpu.queue, &target, &mut rt);
    let pixels = target.read(&gpu.device);
    let at = |x: u32| {
        let i = ((H / 2 * W + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    (at(W / 4), at(3 * W / 4))
}

#[test]
fn a_flattened_diagram_fill_draws_as_its_translucent_colour_does_in_either_blend() {
    let Some(gpu) = Gpu::test_headless() else { return };
    for dark in [true, false] {
        let paint = DiagramStyle::neutral(dark);
        let washes = [("subgraph tint", paint.cluster_fill, paint.surface), ("done task", paint.text.alpha(0.12), paint.surface), ("cylinder lid", paint.text.alpha(0.06), paint.node_fill), ("ink", paint.line, paint.surface)];
        for (what, top, surface) in washes {
            for srgb in [true, false] {
                let flat = rune::theme::with_blend(srgb, || rune::theme::over(top, surface));
                let (translucent, flattened) = drawn(&gpu, Pair { surface, top, flat }, srgb);
                let off = translucent.iter().zip(flattened).map(|(a, b)| a.abs_diff(b)).max().unwrap();
                assert!(off <= 1, "dark {dark}, srgb {srgb}, {what}: translucent draws {translucent:?}, flattened {flattened:?}");
            }
        }
    }
}
