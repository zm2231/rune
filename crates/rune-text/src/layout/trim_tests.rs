use super::*;
use crate::atlas::{COLOR_GLYPH_BUDGET, ColorGlyph, GlyphKey};
use crate::Color;
use crate::{GlyphQuad, Layout};

const PROSE: &str = "Amber lantern orchard quiet harbor, copper meadow signal drift. Pebble willow \
    ember tide under a juniper kestrel; nimbus opal prism. Ffi fjord Wave AVAT 0123456789.";

fn paragraph(ts: &mut TextSystem, size: f32) -> Rc<Layout<Ink>> {
    ts.layout_rich(&PROSE.repeat(4), &[], TextStyle::new(size, 400.0), 2.0, Some(260.0))
}

fn paint(ts: &mut TextSystem, layout: &Rc<Layout<Ink>>) -> Vec<[u32; 6]> {
    let mut out: Vec<GlyphQuad> = Vec::new();
    ts.glyphs_in_band_cached(layout, [8.0, 8.0], 1.0, false, f32::NEG_INFINITY..f32::INFINITY, &mut out);
    out.iter().map(|q| {
        let [x, y, w, h] = q.rect.map(f32::to_bits);
        [x, y, w, h, q.uv[0].to_bits(), q.uv[1].to_bits()]
    }).collect()
}

/// One commit and its frame, painting `layouts`.
fn frame(ts: &mut TextSystem, layouts: &[&Rc<Layout<Ink>>]) {
    ts.begin_frame();
    for layout in layouts { paint(ts, layout); }
    ts.end_commit();
}

/// A layout no commit has used for a while stays shaped, with its glyph run, inside the stale
/// budget. A rest keeps it cached while an element out of sight still holds it, since holding
/// costs nothing more, and its glyph run stays. A trim drops the run, and keeps what the last
/// frame painted.
#[test]
fn a_trim_drops_the_glyph_runs_a_rest_keeps_and_no_frame_paints() {
    let mut ts = TextSystem::default();
    ts.set_glyph_inks(Color::WHITE, &[]);
    let (left, shown) = (paragraph(&mut ts, 13.0), paragraph(&mut ts, 15.0));
    frame(&mut ts, &[&left, &shown]);
    for _ in 0..3 {
        assert!(Rc::ptr_eq(&shown, &paragraph(&mut ts, 15.0)));
        frame(&mut ts, &[&shown]);
    }
    assert_eq!((ts.cached_layout_count(), ts.glyph_cache.len()), (2, 2));
    let bytes = ts.glyph_cache.bytes();

    ts.rest();
    assert_eq!((ts.cached_layout_count(), ts.glyph_cache.len()), (2, 2), "the held layout and its run stay");
    ts.trim_memory();
    assert_eq!(ts.glyph_cache.len(), 1);
    assert!(ts.glyph_cache.bytes() < bytes);
    assert!(Rc::ptr_eq(&shown, &paragraph(&mut ts, 15.0)), "the layout on screen is not shaped again");
    drop(left);
}

/// An atlas page the last frame drew nothing from starts over, and its glyphs draw again as a
/// new text system draws them.
#[test]
fn a_trim_starts_over_the_atlas_pages_the_last_frame_drew_nothing_from() {
    let mut ts = TextSystem::default();
    ts.set_glyph_inks(Color::WHITE, &[]);
    let layout = paragraph(&mut ts, 13.0);
    ts.begin_frame();
    let first = paint(&mut ts, &layout);
    let (entries, rasterized) = (ts.atlas.glyph_entries(), ts.atlas.rasterized());
    assert!(entries > 0);

    // Drawn in the last frame: its page stays, and a replay rasterises nothing.
    ts.rest();
    ts.trim_memory();
    assert_eq!(ts.atlas.glyph_entries(), entries);
    ts.begin_frame();
    assert_eq!(paint(&mut ts, &layout), first);
    assert_eq!(ts.atlas.rasterized(), rasterized);

    // A frame without it, then a trim.
    ts.begin_frame();
    ts.rest();
    ts.trim_memory();
    // Only the glyphs that draw nothing, which have no place on a page, are left.
    let blank = ts.atlas.glyph_entries();
    assert!(blank < entries / 2, "{blank} of {entries}");
    ts.begin_frame();
    assert_eq!(paint(&mut ts, &layout), first);
    assert_eq!(ts.atlas.glyph_entries(), entries);
    assert_eq!(ts.atlas.rasterized(), rasterized + (entries - blank) as u64);
}

/// The atlas gives back the pages past the last one a frame still draws from.
#[test]
fn a_trim_gives_back_the_atlas_pages_after_the_last_in_use() {
    let mut ts = TextSystem::default();
    ts.begin_frame();
    let mask = vec![255u8; 512 * 512];
    while ts.atlas.page_count() < 3 {
        ts.atlas.put(512, 512, &mask, 0, 0).unwrap();
    }
    let first = ts.atlas.page_data(0).unwrap().len();
    ts.rest();
    ts.trim_memory();
    assert_eq!(ts.atlas.page_count(), 3, "pages the frame drew from stay");
    ts.begin_frame();
    ts.atlas.touch(0, 1);
    ts.rest();
    ts.trim_memory();
    assert_eq!(ts.atlas.page_count(), 1);
    // Pages grow by rows as they fill, so only the first page's buffer is compared.
    assert_eq!(ts.atlas.page_data(0).unwrap().len(), first, "the first page keeps its buffer");
}

/// Colour glyphs go at a frame's start only over their budget. A trim retires the ones the
/// last frame didn't draw under it too, and keeps the ones it drew.
#[test]
fn a_trim_retires_the_colour_glyphs_the_last_frame_did_not_draw() {
    let mut ts = TextSystem::default();
    let key = GlyphKey { font: 0, index: 0, glyph: 0, size: 0, coords: 0, subpx: 0, subpy: 0, dilation: 0, apple: 0, skew: 0, bold: false };
    let drawn_key = GlyphKey { glyph: 1, ..key };
    let pixels = rune_image::pixels::Pixels::new();
    pixels.set(1, 1, vec![255; 4]);
    ts.color_pixels = vec![pixels.clone(), pixels];
    let drawn = ColorGlyph { index: 2, used: true, left: 0, top: 0, w: 64, h: 64 };
    ts.color_glyphs.insert(key, ColorGlyph { index: 1, used: false, ..drawn });
    ts.color_glyphs.insert(drawn_key, drawn);
    ts.color_bytes = 2 * 64 * 64 * 4;
    assert!(ts.color_bytes < COLOR_GLYPH_BUDGET);

    ts.rest();
    ts.trim_memory();
    assert!(!ts.color_glyphs.contains_key(&key));
    assert_eq!((ts.color_pixels.len(), ts.color_bytes), (1, 64 * 64 * 4));
    assert_eq!(ts.color_glyphs[&drawn_key].index, 1);
}
