//! Colour glyphs (emoji) as images: a font's COLR v1 paint graph ([`colr`]), or what swash
//! draws of its COLR v0 layers and embedded bitmaps. Each is an RGBA image at the glyph's pixel
//! size with its placement about the pen, which the image atlas keeps.
//!
//! On iOS a glyph whose embedded image swash has no decoder for (`sbix`) is drawn by Core
//! Text instead ([`platform`]), from the font bytes already in memory, and kept the same way.

use rune_image::pixels::Pixels;
use swash::scale::image::Content;
use swash::scale::{Render, Scaler, Source, StrikeWith};
use swash::zeno::Placement;


pub(crate) mod colr;
#[cfg(target_os = "ios")]
mod core_text;
#[cfg(any(test, target_os = "ios"))]
mod faces;
#[cfg(test)]
mod faces_tests;
#[cfg(any(test, target_os = "ios"))]
mod sbix;
#[cfg(test)]
mod sbix_tests;

/// The faces Core Text has opened to draw colour glyphs from, at most `faces::MAX_FACES`.
#[cfg(target_os = "ios")]
pub(crate) type ColorFaces = faces::Faces<core_text::Face>;

/// Off iOS no platform rasterizer is used for colour glyphs, so no face is kept.
#[cfg(not(target_os = "ios"))]
#[derive(Default)]
pub(crate) struct ColorFaces;

/// The colour image of `glyph` (COLR layers or an embedded bitmap scaled to size), or None
/// when the font draws it as an outline.
pub(crate) fn rasterize_color(scaler: &mut Scaler<'_>, glyph: u16) -> Option<(Pixels, Placement)> {
    if !scaler.has_color_bitmaps() && !scaler.has_color_outlines() {
        return None;
    }
    let image = Render::new(&[Source::ColorOutline(0), Source::ColorBitmap(StrikeWith::BestFit)]).render(scaler, glyph)?;
    let (w, h) = (image.placement.width, image.placement.height);
    if image.content != Content::Color || w == 0 || h == 0 || image.data.len() != (w * h * 4) as usize {
        return None;
    }
    let pixels = Pixels::new();
    pixels.set(w, h, image.data);
    Some((pixels, image.placement))
}

/// The colour image of `glyph` of `font` as the platform's text system draws it, `size` px
/// tall, for a glyph [`rasterize_color`] has no image of in a font with colour bitmaps. Only
/// a glyph with an embedded image in a format swash can't decode is asked for; any other is
/// None, and draws as its outline.
#[cfg(target_os = "ios")]
pub(crate) fn platform(faces: &mut ColorFaces, font: &parley::FontData, glyph: u16, size: f32) -> Option<(Pixels, Placement)> {
    if !sbix::needs_platform(font.data.as_ref(), font.index, glyph) {
        return None;
    }
    let face = faces.get((font.data.id(), font.index), || core_text::Face::open(&font.data, font.index))?;
    core_text::rasterize(face, glyph, size)
}

/// Off iOS no platform rasterizer is used: None.
#[cfg(not(target_os = "ios"))]
pub(crate) fn platform(_faces: &mut ColorFaces, _font: &parley::FontData, _glyph: u16, _size: f32) -> Option<(Pixels, Placement)> {
    None
}
