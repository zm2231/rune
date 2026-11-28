use rune::color::{srgb_decode, srgb_encode};
use rune::render::{Gpu, OFFSCREEN_FORMAT, OFFSCREEN_FORMAT_LINEAR, Offscreen, Renderer};
use rune::{Color, Cx, El, Env, Runtime, el, row};
use rune_mermaid::{DiagramStyle, DrawingModel, Hover};

fn render<S: 'static>(gpu: &Gpu, state: S, view: fn(&S, &mut Cx<S>) -> El, size: [u32; 2], srgb: bool) -> Vec<u8> {
    let mut rt = Runtime::new(state, view, (size[0] as f32, size[1] as f32), Env::new(false, 1.0));
    rt.set_srgb_blend(srgb);
    rt.frame(0.0);
    rt.settle_all();
    rt.frame(0.0);
    let format = if srgb { OFFSCREEN_FORMAT } else { OFFSCREEN_FORMAT_LINEAR };
    let mut renderer = Renderer::new(&gpu.device, format);
    renderer.set_srgb_blend(srgb);
    let target = Offscreen::with_format(&gpu.device, size[0], size[1], format);
    renderer.render_runtime_to_offscreen(&gpu.device, &gpu.queue, &target, &mut rt);
    target.read(&gpu.device)
}

fn mixed(top: Color, bottom: Color, srgb: bool) -> Color {
    let m = |t: f32, b: f32| if srgb { srgb_decode(top.a * srgb_encode(t) + (1.0 - top.a) * srgb_encode(b)) } else { top.a * t + (1.0 - top.a) * b };
    Color { r: m(top.r, bottom.r), g: m(top.g, bottom.g), b: m(top.b, bottom.b), a: 1.0 }
}

const W: u32 = 80;
const H: u32 = 40;

#[derive(Clone, Copy)]
struct Pair {
    surface: Color,
    top: Color,
    flat: Color,
}

fn pair(p: &Pair, _: &mut Cx<Pair>) -> El {
    let half = W as f32 / 2.0;
    row().size(W as f32, H as f32).fill(p.surface).child(el().size(half, H as f32).fill(p.top)).child(el().size(half, H as f32).fill(p.flat))
}

#[test]
fn a_flattened_diagram_fill_draws_as_its_translucent_colour_does_in_either_blend() {
    let Some(gpu) = Gpu::test_headless() else { return };
    for dark in [true, false] {
        let paint = DiagramStyle::neutral(dark);
        let washes = [("subgraph tint", paint.cluster_fill, paint.surface), ("done task", paint.text.alpha(0.12), paint.surface), ("cylinder lid", paint.text.alpha(0.06), paint.node_fill), ("ink", paint.line, paint.surface)];
        for (what, top, surface) in washes {
            for srgb in [true, false] {
                let flat = mixed(top, surface, srgb);
                let helper = rune::theme::with_blend(srgb, || rune::theme::over(top, surface));
                let off = [helper.r - flat.r, helper.g - flat.g, helper.b - flat.b].iter().map(|d| d.abs()).fold(0.0, f32::max);
                assert!(off < 1e-5, "dark {dark}, srgb {srgb}, {what}: theme::over gives {helper:?}, source-over gives {flat:?}");
                let pixels = render(&gpu, Pair { surface, top, flat }, pair, [W, H], srgb);
                let at = |x: u32| {
                    let i = ((H / 2 * W + x) * 4) as usize;
                    [pixels[i], pixels[i + 1], pixels[i + 2]]
                };
                let (translucent, flattened) = (at(W / 4), at(3 * W / 4));
                let off = translucent.iter().zip(flattened).map(|(a, b)| a.abs_diff(b)).max().unwrap();
                assert!(off <= 1, "dark {dark}, srgb {srgb}, {what}: translucent draws {translucent:?}, flattened {flattened:?}");
            }
        }
    }
}

const FLOW: &str = "flowchart LR\n  subgraph S [Box]\n    A -->|label| B\n  end\n  B --> C[(Store)]";
const SIZE: [u32; 2] = [520, 200];

fn style() -> DiagramStyle {
    let mut s = DiagramStyle::neutral(true);
    s.max_width = SIZE[0] as f32;
    s
}

fn fresh(_: &(), _: &mut Cx<()>) -> El {
    let paint = style();
    el().size(SIZE[0] as f32, SIZE[1] as f32).fill(paint.surface).child(rune_mermaid::render_drawing(FLOW, &paint, Hover::Edges, paint.text).expect("draws").el)
}

fn prepared(m: &DrawingModel, _: &mut Cx<DrawingModel>) -> El {
    el().size(SIZE[0] as f32, SIZE[1] as f32).fill(style().surface).child(m.draw(|_| Vec::new()).el)
}

#[test]
fn a_prepared_drawing_draws_in_the_blend_of_the_window_showing_it() {
    let Some(gpu) = Gpu::test_headless() else { return };
    for srgb in [true, false] {
        let paint = style();
        let model = rune::theme::with_blend(!srgb, || rune_mermaid::prepare_drawing(FLOW, &paint, Hover::Edges, paint.text)).expect("draws");
        let want = render(&gpu, (), fresh, SIZE, srgb);
        let got = render(&gpu, model, prepared, SIZE, srgb);
        let off = want.iter().zip(&got).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
        assert!(off <= 1, "srgb {srgb}: a drawing prepared in the other blend differs from a fresh one by {off}");
        assert!(want.chunks_exact(4).any(|p| p[..3] != want[..3]), "srgb {srgb}: the diagram drew");
    }
}
