use rune::Color;
use rune::render::{Instance, KIND_GLYPH, KIND_RECT};
use rune_kit::{Kit, KitEnv};

pub struct Glyph {
    pub at: (f32, f32),
    pub fill: [f32; 4],
    pub ink: [f32; 4],
    pub ratio: f32,
}

pub fn built_in_kits() -> Vec<(String, Kit)> {
    let mut out = Vec::new();
    for dark in [false, true] {
        for increased in [false, true] {
            let neutral = Kit::neutral(dark);
            let neutral = if increased { neutral.increased_contrast() } else { neutral };
            let opencode = rune_theme_opencode::kit(&KitEnv::new(dark).srgb_blend(false).increased_contrast(increased));
            let variant = format!("{}{}", if dark { "dark" } else { "light" }, if increased { " increased contrast" } else { "" });
            out.push((format!("neutral {variant}"), neutral));
            out.push((format!("opencode {variant}"), opencode));
        }
    }
    out
}

pub fn text_floor(kit: &Kit) -> f32 {
    if kit.increased_contrast { 7.0 } else { 4.5 }
}

fn contrast(a: [f32; 4], b: [f32; 4]) -> f32 {
    let y = |c: [f32; 4]| Color { r: c[0], g: c[1], b: c[2], a: 1.0 }.luminance();
    let (a, b) = (y(a), y(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn contains(r: [f32; 4], (x, y): (f32, f32)) -> bool {
    x >= r[0] && x <= r[0] + r[2] && y >= r[1] && y <= r[1] + r[3]
}

pub fn glyphs_on_fills(drawn: &[Instance], within: &[[f32; 4]], plain: &[Color]) -> Vec<Glyph> {
    let plain: Vec<[f32; 4]> = plain.iter().map(|c| c.premul(1.0)).collect();
    glyphs_within(drawn, within).into_iter().filter(|g| !plain.contains(&g.fill)).collect()
}

pub fn glyphs_within(drawn: &[Instance], within: &[[f32; 4]]) -> Vec<Glyph> {
    let mut out = Vec::new();
    for (n, glyph) in drawn.iter().enumerate().filter(|(_, i)| i.params[1] == KIND_GLYPH) {
        let at = (glyph.rect[0] + glyph.rect[2] / 2.0, glyph.rect[1] + glyph.rect[3] / 2.0);
        if !within.iter().any(|r| contains(*r, at)) {
            continue;
        }
        let under = drawn[..n].iter().rev().find(|i| i.params[1] == KIND_RECT && i.fill[3] > 0.999 && contains(i.rect, at));
        let Some(fill) = under.map(|i| i.fill) else { continue };
        out.push(Glyph { at, fill, ink: glyph.fill, ratio: contrast(glyph.fill, fill) });
    }
    out
}
