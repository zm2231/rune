//! Colour glyphs (emoji) as images: a font's COLR v1 paint graph ([`colr`]), or what swash
//! draws of its COLR v0 layers and embedded bitmaps. Each is an RGBA image at the glyph's pixel
//! size with its placement about the pen, which the image atlas keeps.

use rune_image::pixels::Pixels;
use swash::scale::image::Content;
use swash::scale::{Render, Scaler, Source, StrikeWith};
use swash::zeno::Placement;

pub(crate) mod colr;

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
