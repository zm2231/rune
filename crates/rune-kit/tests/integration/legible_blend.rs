use rune::color::{hex, srgb_decode};
use rune::render::{Gpu, OFFSCREEN_FORMAT, OFFSCREEN_FORMAT_LINEAR, Offscreen, Renderer};
use rune::text::TextSystem;
use rune::{Color, Cx, El, Env, Runtime, el, text};
use rune_kit::Kit;

const SIZE: u32 = 160;

fn grey() -> Color {
    hex("#808080").unwrap().alpha(0.5)
}

fn ratio(a: Color, b: Color) -> f32 {
    let (a, b) = (a.luminance() + 0.05, b.luminance() + 0.05);
    a.max(b) / a.min(b)
}

fn view(fill: &Color, _: &mut Cx<Color>) -> El {
    let style = rune::TextStyle { size: 120.0, ..Kit::neutral(false).ty.body };
    el().w_full().h_full().fill(*fill).padding(8.0).child(text("M").font(style).color(grey()))
}

fn drawn_ink(gpu: &Gpu, fill: Color, srgb: bool) -> Color {
    let fonts = TextSystem::new(rune_theme_opencode::FONTS, rune_theme_opencode::FAMILIES);
    let mut rt = Runtime::with_text(fill, view, (SIZE as f32, SIZE as f32), Env::new(false, 1.0), fonts);
    rt.set_srgb_blend(srgb);
    rt.frame(0.0);
    rt.settle_all();
    rt.frame(0.0);
    let format = if srgb { OFFSCREEN_FORMAT } else { OFFSCREEN_FORMAT_LINEAR };
    let mut renderer = Renderer::new(&gpu.device, format);
    renderer.set_srgb_blend(srgb);
    let target = Offscreen::with_format(&gpu.device, SIZE, SIZE, format);
    renderer.render_runtime_to_offscreen(&gpu.device, &gpu.queue, &target, &mut rt);
    let pixels = target.read(&gpu.device);
    let decode = |p: &[u8]| Color { r: srgb_decode(p[0] as f32 / 255.0), g: srgb_decode(p[1] as f32 / 255.0), b: srgb_decode(p[2] as f32 / 255.0), a: 1.0 };
    let ink = pixels.chunks_exact(4).map(decode).max_by(|a, b| (a.luminance() - fill.luminance()).abs().total_cmp(&(b.luminance() - fill.luminance()).abs()));
    let ink = ink.unwrap();
    assert!((ink.luminance() - fill.luminance()).abs() > 0.05, "srgb {srgb}: no glyph drawn over {fill:?}");
    ink
}

#[test]
fn translucent_text_is_measured_as_the_renderer_draws_it_in_either_blend() {
    let Some(gpu) = Gpu::test_headless() else { return };
    for srgb in [true, false] {
        for fill in [Color::BLACK, Color::WHITE] {
            let drawn = drawn_ink(&gpu, fill, srgb);
            let measured = rune::theme::with_blend(srgb, || rune_kit::legible(grey(), fill, 1.0));
            let (want, got) = (ratio(drawn, fill), ratio(measured, fill));
            assert!((got / want - 1.0).abs() < 0.02, "srgb {srgb} over {fill:?}: measured {got:.3}:1 ({}), drawn {want:.3}:1 ({})", measured.hex(), drawn.hex());
        }
    }
}
