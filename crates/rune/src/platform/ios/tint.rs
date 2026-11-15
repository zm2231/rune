//! A view's tint as the app's accent. UIKit resolves the view's `tintColor` for a light and a
//! dark appearance and hands over each as four components in extended sRGB: red, green, blue
//! and alpha, where a colour outside sRGB (a Display P3 one) has components below 0 or above 1.
//! Rune's colours are linear sRGB, so each component is clipped to sRGB and decoded. Plain
//! arithmetic, testable on every platform.

use crate::{Color, SystemAccent};

/// A colour's red, green, blue and alpha as `getRed:green:blue:alpha:` wrote them.
pub(crate) type Components = [f64; 4];

/// The colour of `components`, or none for a colour UIKit could not convert (a pattern, a nil
/// tint) or one with a component that is no number.
pub(crate) fn color(components: Option<Components>) -> Option<Color> {
    let [r, g, b, a] = components?;
    if [r, g, b, a].iter().any(|v| !v.is_finite()) { return None; }
    let decode = |v: f64| crate::color::srgb_decode(v.clamp(0.0, 1.0) as f32);
    Some(Color { r: decode(r), g: decode(g), b: decode(b), a: a.clamp(0.0, 1.0) as f32 })
}

/// The accent of a tint resolved for a light and a dark appearance, with focus rings in the
/// same colour at half its alpha. A tint that has no colour in either appearance is the
/// default accent.
pub(crate) fn accent(light: Option<Components>, dark: Option<Components>) -> SystemAccent {
    match (color(light), color(dark)) {
        (Some(light), Some(dark)) => SystemAccent::new(light, dark),
        _ => SystemAccent::DEFAULT,
    }
}

#[cfg(test)]
#[path = "tint_tests.rs"]
mod tests;
