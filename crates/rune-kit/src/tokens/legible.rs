use rune::Color;

use super::palette::{contrast, mix_oklab};

/// `fg` as text on `fill` reading at no less than `min`:1 (WCAG contrast; 4.5 for body text, 7
/// under Increase Contrast). `fill` is the opaque surface the text sits on; its alpha is ignored.
/// Translucent `fg` is laid over `fill` and comes back opaque, as the pixel the window draws:
/// blended in sRGB encoding in a window that blends that way (the default on every backend, as a
/// browser and Core Animation blend), and in linear light in one that sets
/// `App::srgb_blend(false)`. The mode is [`rune::theme::blends_srgb`]'s: the window's while a view
/// is built and a [`KitEnv`](crate::KitEnv)'s while its kit is, and sRGB outside both, so a
/// linear-light window that calls this elsewhere wraps the call in
/// [`rune::theme::with_blend`]. Text drawn straight onto a transparent part of a translucent
/// window blends over whatever the OS composites behind it, which `fill` can't stand for.
///
/// An opaque colour that already reads at `min` comes back as it is; one that falls short moves
/// in OKLab toward black or white, whichever can reach `min`, keeping its hue as far as it can.
/// When neither black nor white reaches `min` on `fill`, the one with more contrast comes back.
///
/// ```
/// use rune::{Color, color::hex};
/// let grey = hex("#8a8a8a").unwrap();
/// let text = rune_kit::legible(grey, Color::WHITE, 4.5);
/// assert!(text.luminance() < grey.luminance());
/// assert_eq!(rune_kit::legible(Color::BLACK, Color::WHITE, 4.5), Color::BLACK);
/// ```
pub fn legible(fg: Color, fill: Color, min: f32) -> Color {
    let fg = opaque_over(fg, fill);
    if contrast(fg, fill) >= min {
        return fg;
    }
    let pole = |dark: bool| if dark { Color::BLACK } else { Color::WHITE };
    let own_side = fg.luminance() <= fill.luminance();
    let dark = if contrast(pole(own_side), fill) >= min.min(contrast(pole(!own_side), fill)) { own_side } else { !own_side };
    let end = pole(dark);
    if contrast(end, fill) < min {
        return end;
    }
    let at = |t: f32| mix_oklab(fg, end, t);
    let mut low = 0.0;
    if dark != own_side {
        let mut high = 1.0;
        for _ in 0..24 {
            let mid = (low + high) * 0.5;
            if (at(mid).luminance() <= fill.luminance()) == dark {
                high = mid;
            } else {
                low = mid;
            }
        }
    }
    let goal = (min * 1.01).min(contrast(end, fill));
    let mut high = 1.0;
    for _ in 0..24 {
        let mid = (low + high) * 0.5;
        if contrast(at(mid), fill) >= goal {
            high = mid;
        } else {
            low = mid;
        }
    }
    at(high)
}

/// `fg` as text that reads at no less than `min`:1 on every one of `fills` at once: one colour
/// for text that sits on several surfaces, such as a page and the panes over it. A colour that
/// already reads on all of them comes back as it is, translucent or not. One that falls short
/// comes back opaque (laid over the first fill), moved in OKLab toward black or white along
/// its hue, whichever reaches the floor on every fill with the smaller move. When the fills
/// mix light and dark, the floor can only be met between them, or not at all; with no colour
/// reaching `min` on all of them, the one whose lowest contrast is highest comes back. No
/// fills gives `fg` back. Fills and translucent text are measured as [`legible`] measures them.
///
/// ```
/// use rune::{Color, color::hex};
/// let fills = [Color::BLACK, hex("#5a5a5a").unwrap()];
/// let text = rune_kit::legible_on_all(hex("#1e1e1e").unwrap(), &fills, 3.0);
/// assert!(text.luminance() > hex("#5a5a5a").unwrap().luminance());
/// ```
pub fn legible_on_all(fg: Color, fills: &[Color], min: f32) -> Color {
    let Some(&first) = fills.first() else {
        return fg;
    };
    if lowest_contrast(fg, fills) >= min {
        return fg;
    }
    let start = opaque_over(fg, first);
    let lum: Vec<f32> = fills.iter().map(|fill| fill.luminance()).collect();
    let goal = min * 1.01;
    let reach = if windows(&lum, goal).is_empty() { highest_reach(&lum, goal) } else { goal };
    let y = start.luminance();
    let open = windows(&lum, reach);
    if open.iter().any(|&(lo, hi)| (lo..=hi).contains(&y)) {
        return start;
    }
    let darker = open.iter().map(|w| w.1).filter(|&hi| hi < y).reduce(f32::max);
    let lighter = open.iter().map(|w| w.0).filter(|&lo| lo > y).reduce(f32::min);
    let darker = darker.map(|hi| toward(start, Color::BLACK, |c| c.luminance() <= hi));
    let lighter = lighter.map(|lo| toward(start, Color::WHITE, |c| c.luminance() >= lo));
    let chosen = match (darker, lighter) {
        (Some(d), Some(l)) => {
            if distance(start, d) <= distance(start, l) {
                d
            } else {
                l
            }
        }
        (Some(c), None) | (None, Some(c)) => c,
        (None, None) => start,
    };
    if lowest_contrast(chosen, fills) >= min {
        return chosen;
    }
    let settled = settle(chosen, fills, min);
    if lowest_contrast(settled, fills) > lowest_contrast(chosen, fills) { settled } else { chosen }
}

fn settle(c: Color, fills: &[Color], min: f32) -> Color {
    let y = c.luminance();
    let meets = |k: f32, darker: bool| {
        let c = scaled(c, k);
        fills.iter().filter(|f| (f.luminance() < y) == darker).all(|&f| contrast(c, f) >= min)
    };
    let darker = !meets(1.0, true);
    let (mut low, mut high) = if darker { (1.0, 1.0 + SETTLE_SPAN) } else { (1.0 - SETTLE_SPAN, 1.0) };
    for _ in 0..64 {
        let mid = (low + high) * 0.5;
        if mid == low || mid == high {
            break;
        }
        if meets(mid, darker) == darker {
            high = mid;
        } else {
            low = mid;
        }
    }
    let bracket = [scaled(c, low), scaled(c, high)];
    let nudged = bracket.into_iter().flat_map(|c| (0..3).flat_map(move |channel| (-4..=4).map(move |ulps| nudge(c, channel, ulps))));
    nudged.fold(bracket[usize::from(darker)], |best, c| {
        if lowest_contrast(c, fills) > lowest_contrast(best, fills) { c } else { best }
    })
}

fn nudge(c: Color, channel: usize, ulps: i32) -> Color {
    let step = |v: f32| f32::from_bits(v.to_bits().saturating_add_signed(ulps)).clamp(0.0, 1.0);
    match channel {
        0 => Color { r: step(c.r), ..c },
        1 => Color { g: step(c.g), ..c },
        _ => Color { b: step(c.b), ..c },
    }
}

pub(crate) fn lowest_contrast(fg: Color, fills: &[Color]) -> f32 {
    fills.iter().map(|&fill| contrast(opaque_over(fg, fill), fill)).fold(f32::INFINITY, f32::min)
}

const SETTLE_SPAN: f32 = 1e-4;

fn scaled(c: Color, by: f32) -> Color {
    Color { r: (c.r * by).min(1.0), g: (c.g * by).min(1.0), b: (c.b * by).min(1.0), a: c.a }
}

pub(crate) fn opaque_over(fg: Color, fill: Color) -> Color {
    if fg.a < 1.0 { crate::over(fg, fill.alpha(1.0)) } else { fg }
}

fn windows(lum: &[f32], min: f32) -> Vec<(f32, f32)> {
    let mut sorted = lum.to_vec();
    sorted.sort_by(f32::total_cmp);
    let mut out = Vec::new();
    let mut lo: f32 = 0.0;
    for y in sorted {
        let hi = ((y + 0.05) / min - 0.05).min(1.0);
        if lo <= hi {
            out.push((lo, hi));
        }
        lo = lo.max((y + 0.05) * min - 0.05);
    }
    if lo <= 1.0 {
        out.push((lo, 1.0));
    }
    out
}

fn highest_reach(lum: &[f32], cap: f32) -> f32 {
    let (mut low, mut high) = (1.0, cap);
    for _ in 0..24 {
        let mid = (low + high) * 0.5;
        if windows(lum, mid).is_empty() {
            high = mid;
        } else {
            low = mid;
        }
    }
    low
}

fn toward(from: Color, end: Color, done: impl Fn(Color) -> bool) -> Color {
    let at = |t: f32| mix_oklab(from, end, t);
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..24 {
        let mid = (low + high) * 0.5;
        if done(at(mid)) {
            high = mid;
        } else {
            low = mid;
        }
    }
    at(high)
}

fn distance(a: Color, b: Color) -> f32 {
    let (a, b) = (a.to_oklab(), b.to_oklab());
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}
