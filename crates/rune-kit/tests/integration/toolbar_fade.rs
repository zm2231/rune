//! A toolbar's trailing items that don't fit fade out toward the title rather than stopping at a
//! hard edge, at every kit text scale.

use rune::render::{Gpu, OFFSCREEN_FORMAT, Offscreen, Renderer};
use rune::text::TextSystem;
use rune::{Color, Cx, El, Env, Runtime, col, el, text};
use rune_kit::shell::Toolbar;
use rune_kit::*;

const W: u32 = 420;
const H: u32 = 120;

fn view(scale: &f32, _: &mut Cx<f32>) -> El {
    let k = Kit::neutral(true).with_text_scale(*scale);
    let bar = Toolbar::new()
        .leading(text("Quarterly planning notes for the design review").key("title").font(k.ty.ui_medium).truncate().shrink().min_w(0.0))
        .trailing(el().key("wide").size(600.0, 24.0).fill(Color::WHITE))
        .build(&k);
    col().w_full().h_full().fill(Color::BLACK).child(bar)
}

fn red(px: &[u8], x: f32, y: f32) -> u8 {
    px[((y as u32 * W + x as u32) * 4) as usize]
}

#[test]
fn overflowing_trailing_items_fade_out_clear_of_the_title() {
    let Some(gpu) = Gpu::test_headless() else { return };
    for scale in [1.0, 1.6, 2.5] {
        let text = TextSystem::new(rune_theme_opencode::FONTS, rune_theme_opencode::FAMILIES);
        let mut rt = Runtime::with_text(scale, view, (W as f32, H as f32), Env::new(true, 1.0), text);
        rt.frame(0.0);
        rt.settle_all();
        rt.frame(0.0);
        let rect = |key: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no {key}")).rect;
        let (title, wide) = (rect("title"), rect("wide"));
        let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
        let target = Offscreen::new(&gpu.device, W, H);
        renderer.render_runtime_to_offscreen(&gpu.device, &gpu.queue, &target, &mut rt);
        let px = target.read(&gpu.device);

        let y = wide[1] + wide[3] / 2.0;
        let edge = title[0] + title[2];
        let fade = (16.0 * scale).max(16.0);
        assert!(red(&px, edge - 1.0, y) < 8, "{scale}: the trailing item paints under the title");
        let ramp: Vec<u8> = (0..fade as usize).map(|i| red(&px, edge + i as f32 + 0.5, y)).collect();
        assert!(ramp.windows(2).all(|w| w[1] + 2 >= w[0]), "{scale}: the fade isn't a ramp: {ramp:?}");
        assert!(ramp[0] < 40 && ramp[ramp.len() - 1] > 200, "{scale}: no fade from the title's edge: {ramp:?}");
        let partial = ramp.iter().filter(|&&v| (40..200).contains(&v)).count();
        assert!(partial as f32 >= fade / 3.0, "{scale}: the edge is hard, not faded: {ramp:?}");
    }
}
