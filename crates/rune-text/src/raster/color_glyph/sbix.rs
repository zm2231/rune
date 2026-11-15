//! Whether a font's `sbix` table holds a glyph's image in a format swash can't draw. swash
//! decodes PNG images (`png `); Apple's emoji font on iOS holds its images in a compressed
//! format of Apple's own (`emjc`), which only Core Text draws.

/// A glyph's image is a PNG.
const PNG: [u8; 4] = *b"png ";
/// A glyph's image is another glyph's, whose id follows the tag.
const DUPE: [u8; 4] = *b"dupe";

fn bytes<const N: usize>(table: &[u8], at: usize) -> Option<[u8; N]> {
    table.get(at..at.checked_add(N)?)?.try_into().ok()
}

fn u16_at(table: &[u8], at: usize) -> Option<u16> {
    bytes(table, at).map(u16::from_be_bytes)
}

fn u32_at(table: &[u8], at: usize) -> Option<usize> {
    bytes(table, at).map(|b| u32::from_be_bytes(b) as usize)
}

/// Where `glyph`'s image starts in the strike at `strike`, and its format tag: None when the
/// strike has no image for it. A strike is its ppem and ppi, then an offset per glyph and one
/// past the last; an image is its origin (two `i16`s), its tag, then its data.
fn image(sbix: &[u8], strike: usize, glyph: u16) -> Option<(usize, [u8; 4])> {
    let offsets = strike.checked_add(4 + glyph as usize * 4)?;
    let (start, end) = (u32_at(sbix, offsets)?, u32_at(sbix, offsets.checked_add(4)?)?);
    if end <= start {
        return None;
    }
    let at = strike.checked_add(start)?;
    Some((at, bytes(sbix, at.checked_add(4)?)?))
}

/// Whether some strike of `sbix`, a font's whole `sbix` table, holds an image for `glyph` in a
/// format other than PNG. A glyph that stands for another's image is followed one step, within
/// its strike, as swash follows it; one that still stands for another after that step counts
/// as such an image, since swash goes no further. A strike with no image for the glyph, or
/// one cut short, counts for nothing.
///
/// Every strike is looked at, whatever its size: this is asked only of a glyph swash drew no
/// image for, so which strike a size would pick doesn't matter, only whether any has an image
/// that explains it.
pub(super) fn foreign_image(sbix: &[u8], glyph: u16) -> bool {
    let Some(strikes) = u32_at(sbix, 4) else { return false };
    for i in 0..strikes {
        // Past the table's end: no strike after this one is in it either.
        let Some(strike) = i.checked_mul(4).and_then(|at| at.checked_add(8)).and_then(|at| u32_at(sbix, at)) else { break };
        let Some((at, tag)) = image(sbix, strike, glyph) else { continue };
        let tag = if tag == DUPE {
            let Some(other) = at.checked_add(8).and_then(|at| u16_at(sbix, at)) else { continue };
            let Some((_, tag)) = image(sbix, strike, other) else { continue };
            tag
        } else {
            tag
        };
        if tag != PNG {
            return true;
        }
    }
    false
}

/// Whether `glyph` of face `index` of the font file `data` has an image that swash can't draw
/// and the platform's text system may: one in an `sbix` strike, in a format other than PNG.
pub(super) fn needs_platform(data: &[u8], index: u32, glyph: u16) -> bool {
    let Ok(font) = skrifa::FontRef::from_index(data, index) else { return false };
    let Some(table) = font.table_data(skrifa::Tag::new(b"sbix")) else { return false };
    foreign_image(table.as_bytes(), glyph)
}
