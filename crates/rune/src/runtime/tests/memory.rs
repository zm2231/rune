use super::*;
use crate::pixels::{ImageAtlas, Pixels};

const PROSE: &str = "Amber lantern orchard quiet harbor, copper meadow signal drift. Pebble willow \
    ember tide under a juniper kestrel; nimbus opal prism and a quiet harbor beneath the lantern. ";

/// A page of paragraphs, or one line once the reader has left it.
fn page(reading: &bool, _: &mut Cx<bool>) -> El {
    if !*reading {
        return col().size(320.0, 240.0).child(crate::text("Done").key("done").color(crate::Color::WHITE));
    }
    col().size(320.0, 240.0).children((0..12).map(|i| {
        crate::text(format!("{i}. {}", PROSE.repeat(1 + i % 3))).key(format!("p{i}"))
            .font(crate::TextStyle::new(13.0, 400.0)).color(crate::Color::WHITE).w_full()
    }))
}

fn instance_bytes(rt: &Runtime<bool>) -> Vec<u8> {
    bytemuck::cast_slice(&rt.instances).to_vec()
}

fn show(rt: &mut Runtime<bool>, reading: bool) {
    *rt.state = reading;
    rt.invalidate();
    rt.frame(1.0 / 120.0);
}

/// A trim takes nothing the frame on screen draws from: its layouts and its glyphs' masks
/// stay, the view isn't re-run, and the next frame draws the same instances and neither
/// shapes a text nor rasterises a glyph again.
#[test]
fn a_frame_after_a_trim_draws_what_it_drew_and_makes_nothing_again() {
    let mut rt = Runtime::new(true, page, (320.0, 240.0), Env::new(true, 2.0));
    rt.frame(0.0);
    rt.frame(1.0 / 120.0);
    let drawn = instance_bytes(&rt);
    let made = (rt.text.shaped, rt.text.atlas.rasterized(), rt.text.atlas.glyph_entries(), rt.text.glyph_cache.len(), rt.text.cached_layout_count());
    assert!(made.2 > 0 && made.3 > 0 && made.4 > 0);

    rt.trim_memory();
    assert_eq!((rt.text.atlas.glyph_entries(), rt.text.cached_layout_count()), (made.2, made.4));
    assert_eq!(instance_bytes(&rt), drawn);
    let commits = rt.commit.commits;
    assert!(!rt.frame(1.0 / 120.0), "a trim asks for no frame");
    assert_eq!(rt.commit.commits, commits, "and doesn't re-run the view");
    assert_eq!(instance_bytes(&rt), drawn);
    assert_eq!((rt.text.shaped, rt.text.atlas.rasterized()), (made.0, made.1));
}

/// A view the reader left keeps its layouts and glyph runs for its return. A trim frees
/// them, and the view is shaped again when it returns, into what a new window draws.
#[test]
fn a_trim_frees_the_text_a_view_left_behind() {
    let mut rt = Runtime::new(true, page, (320.0, 240.0), Env::new(true, 2.0));
    rt.frame(0.0);
    show(&mut rt, false);
    // The page's exit ends, and its elements let go of their layouts.
    for _ in 0..240 { rt.frame(1.0 / 120.0); }
    let mut left = Runtime::new(false, page, (320.0, 240.0), Env::new(true, 2.0));
    left.frame(0.0);
    let kept = (rt.text.cached_layout_count(), rt.text.glyph_cache.len(), rt.text.glyph_cache.bytes(), rt.text.atlas.glyph_entries());
    let needed = (left.text.cached_layout_count(), left.text.glyph_cache.len(), left.text.glyph_cache.bytes(), left.text.atlas.glyph_entries());
    assert!(kept.0 > needed.0 && kept.1 > needed.1 && kept.2 > needed.2, "{kept:?} {needed:?}");

    let (drawn, commits) = (instance_bytes(&rt), rt.commit.commits);
    rt.trim_memory();
    assert_eq!((instance_bytes(&rt), rt.commit.commits), (drawn.clone(), commits));
    assert_eq!(rt.text.cached_layout_count(), needed.0);
    assert!(rt.text.glyph_cache.len() <= needed.1 && rt.text.glyph_cache.bytes() < kept.2);
    let left_behind = (rt.text.cached_layout_count(), rt.text.glyph_cache.len(), rt.text.glyph_cache.bytes());
    rt.trim_memory();
    assert_eq!((rt.text.cached_layout_count(), rt.text.glyph_cache.len(), rt.text.glyph_cache.bytes()), left_behind, "a second trim finds nothing");
    // The line on screen draws from the page the paragraphs' masks share, so the page stays.
    assert_eq!(rt.text.atlas.glyph_entries(), kept.3);
    rt.frame(1.0 / 120.0);
    assert_eq!((instance_bytes(&rt), rt.commit.commits), (drawn, commits));

    let shaped = rt.text.shaped;
    show(&mut rt, true);
    for _ in 0..240 { rt.frame(1.0 / 120.0); }
    assert!(rt.text.shaped > shaped, "the page is shaped again");
    let mut fresh = Runtime::new(true, page, (320.0, 240.0), Env::new(true, 2.0));
    fresh.frame(0.0);
    assert_eq!(rt.instances.len(), fresh.instances.len());
    assert_eq!(rt.text.cached_layout_count(), fresh.text.cached_layout_count() + needed.0);
}

fn image(shade: u8, side: u32) -> Pixels {
    let pixels = Pixels::new();
    pixels.set(side, side, vec![shade; (side * side * 4) as usize]);
    pixels
}

fn atlas_bytes(atlas: &ImageAtlas) -> usize {
    atlas.pages.iter().map(|page| page.data.len()).sum()
}

/// The image atlas keeps its first page for a view come back to. A rest, which a trim runs,
/// frees every page the last frame showed nothing from, and an image shown again is copied in
/// from its pixels.
#[test]
fn a_trim_frees_the_image_pages_the_last_frame_showed_nothing_from() {
    // The photo outgrows a shelf page, so it takes one of its own, and fits the smallest texture
    // side any GPU device here may cap the atlas at.
    let (icon, photo) = (image(7, 64), image(9, 2048));
    let mut atlas = ImageAtlas::default();
    let placed = atlas.place(&icon, [64.0; 2]).unwrap();
    atlas.place(&photo, [2048.0; 2]).unwrap();
    atlas.end_frame();
    let both = atlas_bytes(&atlas);

    // On screen: nothing goes.
    atlas.rest();
    assert_eq!(atlas_bytes(&atlas), both);

    // The photo leaves; its page has gone already, as every frame's end frees it.
    atlas.place(&icon, [64.0; 2]).unwrap();
    atlas.end_frame();
    let first = atlas_bytes(&atlas);
    assert!(first > 0 && first < both);
    atlas.rest();
    assert_eq!(atlas_bytes(&atlas), first, "the icon on screen keeps its page");
    assert_eq!(atlas.place(&icon, [64.0; 2]), Some(placed));
    atlas.end_frame();

    // The icon leaves: the first page stays for its return, until a trim.
    atlas.end_frame();
    assert_eq!(atlas_bytes(&atlas), first);
    atlas.rest();
    assert_eq!((atlas_bytes(&atlas), atlas.pages.len()), (0, 0));

    let again = atlas.place(&icon, [64.0; 2]).unwrap();
    assert_eq!(again, placed);
    let page = &atlas.pages[again.page];
    assert_eq!(page.data[..4], [7; 4]);
    assert!(page.dirty.is_some(), "the copy is uploaded");
}

/// A runtime that has drawn no frame has nothing to free.
#[test]
fn a_trim_before_the_first_frame_is_a_no_op() {
    let mut rt = Runtime::new(true, page, (320.0, 240.0), Env::new(true, 2.0));
    rt.trim_memory();
    assert_eq!(rt.commit.commits, 0);
    rt.frame(0.0);
    let mut fresh = Runtime::new(true, page, (320.0, 240.0), Env::new(true, 2.0));
    fresh.frame(0.0);
    assert_eq!(instance_bytes(&rt), instance_bytes(&fresh));
}

struct Chat { draft: crate::edit::TextEdit, sent: bool }

fn chat(s: &Chat, _: &mut Cx<Chat>) -> El {
    let mut rows = col().w_full().child(el().key("earlier").w_full().h(80.0));
    if s.sent { rows = rows.child(col().key("bubble").w_full().py(8.0).child(crate::el::text("hello flight").key("destination").wrap())); }
    col().w_full().h_full()
        .child(col().key("scroll").w_full().grow(1.0).shrink().child(rows))
        .child(crate::el::text_input(&s.draft, crate::tok::TextStyle::new(14.0, 400.0)).key("source").w_full().h(60.0)
            .on_edit(|s: &mut Chat| &mut s.draft)
            .on_submit(|s: &mut Chat, tx: &mut Tx| {
                s.sent = true;
                s.draft.clear();
                tx.fly_text("source", "destination");
            }))
}

/// A trim while a text flies takes none of its glyphs: every frame of the flight and after
/// it draws the instances an untrimmed run draws.
#[test]
fn a_trim_during_a_text_flight_changes_no_frame_of_it() {
    let mut runs: Vec<Runtime<Chat>> = (0..2).map(|_| {
        let mut draft = crate::edit::TextEdit::new("hello flight");
        draft.focused = true;
        let mut rt = Runtime::new(Chat { draft, sent: false }, chat, (400.0, 300.0), Env::new(true, 2.0));
        rt.frame(0.0);
        rt.key(KeyEvent { key: crate::edit::Key::Enter, mods: Default::default() });
        for _ in 0..3 { rt.frame(1.0 / 120.0); }
        assert_eq!(rt.motion.flights().len(), 1);
        rt
    }).collect();
    let (plain, mut trimmed) = (runs.pop().unwrap(), runs.pop().unwrap());
    let mut plain = plain;
    trimmed.trim_memory();
    assert_eq!(trimmed.motion.flights().len(), 1);
    for frame in 0..360 {
        trimmed.frame(1.0 / 120.0);
        plain.frame(1.0 / 120.0);
        assert!(bytemuck::cast_slice::<_, u8>(&trimmed.instances) == bytemuck::cast_slice::<_, u8>(&plain.instances), "frame {frame}");
    }
    assert!(trimmed.motion.flights().is_empty(), "the flight landed");
}
