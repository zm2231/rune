use rune::Color;

use super::palette::{contrast, mix_oklab};

/// `fg` as text on `fill` reading at no less than `min`:1 (WCAG contrast; 4.5 for body text, 7
/// under Increase Contrast). Translucent `fg` is laid over `fill` and comes back opaque. An
/// opaque colour that already reads at `min` comes back as it is; one that falls short moves in
/// OKLab toward black or white, whichever can reach `min`, keeping its hue as far as it can.
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
/// fills gives `fg` back.
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
    if fills.iter().all(|&fill| contrast(fill.mix(fg.alpha(1.0), fg.a), fill) >= min) {
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
    match (darker, lighter) {
        (Some(d), Some(l)) => {
            if distance(start, d) <= distance(start, l) {
                d
            } else {
                l
            }
        }
        (Some(c), None) | (None, Some(c)) => c,
        (None, None) => start,
    }
}

pub(crate) fn opaque_over(fg: Color, fill: Color) -> Color {
    if fg.a < 1.0 { Color { a: 1.0, ..fill.mix(fg, fg.a) } } else { fg }
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
