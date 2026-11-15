use super::sbix::*;

/// An `sbix` table: each strike is its ppem and, by glyph id, the image's tag and data (None
/// is a glyph with no image).
fn table(strikes: &[(u16, &[Option<(&[u8; 4], &[u8])>])]) -> Vec<u8> {
    // Version, flags, the strike count, then an offset per strike.
    let mut out = vec![0, 1, 0, 1];
    out.extend((strikes.len() as u32).to_be_bytes());
    let mut at = 8 + strikes.len() * 4;
    let bodies: Vec<Vec<u8>> = strikes.iter().map(|&(ppem, glyphs)| strike(ppem, glyphs)).collect();
    for body in &bodies {
        out.extend((at as u32).to_be_bytes());
        at += body.len();
    }
    out.extend(bodies.concat());
    out
}

/// One strike: ppem, ppi, an offset per glyph and one past the last, then the images.
fn strike(ppem: u16, glyphs: &[Option<(&[u8; 4], &[u8])>]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(ppem.to_be_bytes());
    out.extend(72u16.to_be_bytes());
    let mut at = 4 + (glyphs.len() + 1) * 4;
    let mut images = Vec::new();
    for glyph in glyphs {
        out.extend((at as u32).to_be_bytes());
        if let Some((tag, data)) = glyph {
            // The image's origin, its tag, its data.
            images.extend([0, 0, 0, 0]);
            images.extend(*tag);
            images.extend(*data);
            at += 8 + data.len();
        }
    }
    out.extend((at as u32).to_be_bytes());
    out.extend(images);
    out
}

#[test]
fn an_image_that_is_not_a_png_is_foreign() {
    let sbix = table(&[(160, &[None, Some((b"emjc", &[1, 2, 3])), Some((b"png ", &[4, 5])), Some((b"jpg ", &[6]))])]);
    assert!(foreign_image(&sbix, 1));
    assert!(!foreign_image(&sbix, 2), "swash draws a PNG");
    assert!(foreign_image(&sbix, 3));
}

#[test]
fn a_glyph_with_no_image_has_none_that_is_foreign() {
    let sbix = table(&[(160, &[None, Some((b"emjc", &[1]))])]);
    assert!(!foreign_image(&sbix, 0));
    // Past the strike's last glyph: its offsets are off the table.
    assert!(!foreign_image(&sbix, 400));
    assert!(!foreign_image(&sbix, u16::MAX));
}

#[test]
fn a_font_of_pngs_has_no_foreign_image() {
    let sbix = table(&[
        (20, &[Some((b"png ", &[1])), Some((b"png ", &[2]))]),
        (40, &[Some((b"png ", &[3])), Some((b"dupe", &[0, 0]))]),
        (160, &[Some((b"png ", &[5])), None]),
    ]);
    assert!(!foreign_image(&sbix, 0));
    assert!(!foreign_image(&sbix, 1));
}

#[test]
fn any_strike_with_a_foreign_image_counts() {
    // The small strike, which a small text would be drawn from, has no image for glyph 0 and
    // a PNG for glyph 1; a larger one has the image swash can't draw.
    let sbix = table(&[
        (20, &[None, Some((b"png ", &[1]))]),
        (40, &[Some((b"png ", &[1])), Some((b"png ", &[1]))]),
        (160, &[Some((b"emjc", &[1])), Some((b"emjc", &[1]))]),
    ]);
    assert!(foreign_image(&sbix, 0));
    assert!(foreign_image(&sbix, 1));
    // The other way about: the foreign image is in the first strike.
    let sbix = table(&[(20, &[Some((b"emjc", &[1]))]), (160, &[Some((b"png ", &[1]))])]);
    assert!(foreign_image(&sbix, 0));
}

#[test]
fn a_glyph_standing_for_another_is_followed_one_step() {
    let sbix = table(&[(
        160,
        &[
            Some((b"dupe", &[0, 2])),
            Some((b"dupe", &[0, 0])),
            Some((b"emjc", &[9])),
            Some((b"dupe", &[0, 4])),
            None,
            Some((b"dupe", &[0, 6])),
            Some((b"png ", &[7])),
            Some((b"dupe", &[0, 5])),
            Some((b"dupe", &[1, 0])),
        ],
    )]);
    assert!(foreign_image(&sbix, 0), "it stands for an image swash can't draw");
    // Glyph 1 stands for glyph 0, which stands for glyph 2: swash stops after one step, so
    // the glyph is the platform's to draw.
    assert!(foreign_image(&sbix, 1));
    assert!(!foreign_image(&sbix, 3), "the glyph it stands for has no image");
    assert!(!foreign_image(&sbix, 5), "it stands for a PNG");
    assert!(foreign_image(&sbix, 7), "two steps from a PNG is past where swash goes");
    assert!(!foreign_image(&sbix, 8), "it stands for a glyph the strike doesn't have");
}

#[test]
fn a_cut_or_empty_table_has_no_foreign_image() {
    let sbix = table(&[(160, &[Some((b"emjc", &[1, 2, 3]))])]);
    assert!(foreign_image(&sbix, 0));
    // The tag's last byte is three bytes of data from the end.
    for len in 0..sbix.len() - 3 {
        assert!(!foreign_image(&sbix[..len], 0), "cut to {len} bytes");
    }
    // A stand-in whose glyph id is cut off.
    let stand_in = table(&[(160, &[Some((b"dupe", &[0, 1])), Some((b"emjc", &[1]))])]);
    assert!(foreign_image(&stand_in, 0));
    let id = stand_in.len() - 9 - 2;
    assert_eq!(stand_in[id - 4..id], *b"dupe");
    assert!(!foreign_image(&stand_in[..id + 1], 0));
    // A strike count far past the table's end stops at the first offset that isn't there.
    let mut many = sbix.clone();
    many[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(foreign_image(&many, 0));
    let mut pngs = table(&[(160, &[Some((b"png ", &[1, 2, 3]))])]);
    pngs[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(!foreign_image(&pngs, 0));
}

/// A font file with one table, `sbix`.
fn font(sbix: &[u8]) -> Vec<u8> {
    // sfnt version 1.0, one table, then searchRange, entrySelector and rangeShift.
    let mut out = vec![0, 1, 0, 0, 0, 1, 0, 16, 0, 0, 0, 0];
    out.extend(*b"sbix");
    out.extend(0u32.to_be_bytes());
    out.extend(28u32.to_be_bytes());
    out.extend((sbix.len() as u32).to_be_bytes());
    out.extend(sbix);
    out
}

#[test]
fn a_font_needs_the_platform_for_an_image_swash_cannot_draw() {
    let data = font(&table(&[(160, &[None, Some((b"emjc", &[1, 2, 3])), Some((b"png ", &[4, 5]))])]));
    assert!(needs_platform(&data, 0, 1));
    assert!(!needs_platform(&data, 0, 2), "swash draws a PNG");
    assert!(!needs_platform(&data, 0, 0), "a glyph with no image is an outline");
    assert!(!needs_platform(&data, 3, 1), "no such face");
    assert!(!needs_platform(&[], 0, 1));
}
