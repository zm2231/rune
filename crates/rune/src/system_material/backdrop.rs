use super::SystemMaterial;
use crate::Color;

#[cfg(test)]
#[path = "backdrop_tests.rs"]
mod tests;

pub(crate) fn blurs(fill: Color) -> bool { fill.a < 1.0 }

pub(crate) fn sigma(material: SystemMaterial) -> f32 {
    match material {
        SystemMaterial::Glass => 12.0,
        SystemMaterial::Popover => 20.0,
        SystemMaterial::Sidebar => 24.0,
    }
}

pub(crate) fn tint(material: SystemMaterial, fill: Color, dark: bool) -> Color {
    if fill.a > 0.0 { return fill; }
    let (white, alpha) = match (material, dark) {
        (SystemMaterial::Glass, false) => (1.0, 0.22),
        (SystemMaterial::Glass, true) => (0.0, 0.22),
        (SystemMaterial::Popover, false) => (0.98, 0.62),
        (SystemMaterial::Popover, true) => (0.13, 0.62),
        (SystemMaterial::Sidebar, false) => (0.94, 0.72),
        (SystemMaterial::Sidebar, true) => (0.11, 0.72),
    };
    let level = crate::color::srgb_decode(white);
    Color { r: level, g: level, b: level, a: alpha }
}
