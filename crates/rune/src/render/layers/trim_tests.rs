use super::super::tests::{pixels, rect};
use super::*;
use crate::el::ScrollSpec;
use crate::pixels::Pixels;
use crate::{Color, Cx, El, Env, Runtime, col, el};

/// A list's cached rows (`El::cache`, one node a row), 64×16 each, the first `count` of them.
fn rows(count: u64) -> Vec<Instance> {
    (0..count).flat_map(|row| {
        let bounds = [0.0, row as f32 * 16.0, 64.0, 16.0];
        let mut scope = Instance::begin_layer(bounds, NO_CLIP, LayerSpec::default());
        scope.params[3] = 1.0;
        scope.radii = pack_u64_f32(row);
        scope.uv = pack_u64_f32(1);
        [scope, rect([8.0, bounds[1] + 4.0, 32.0 + row as f32 * 2.0, 8.0], [1.0, 0.5, 0.25, 1.0]), Instance::end_layer()]
    }).collect()
}

/// Two cached rows under a half-opaque menu, which needs a target of its own.
fn rows_and_menu() -> Vec<Instance> {
    let bounds = [40.0, 4.0, 20.0, 24.0];
    let mut instances = rows(2);
    instances.extend([Instance::begin_layer(bounds, NO_CLIP, LayerSpec { opacity: 0.5, ..Default::default() }),
        rect(bounds, [1.0; 4]), Instance::end_layer()]);
    instances
}

fn pooled_bytes(renderer: &Renderer) -> u64 {
    renderer.layers.retained.iter().chain(&renderer.layers.spare).map(Renderer::target_bytes).sum()
}

/// A trim frees every cached row the frame doesn't show and every spare target, tells the atlas
/// which caches are left, and leaves the frame on screen as it was. The rows it freed draw
/// again as a renderer that never had them draws them.
#[test]
fn a_trim_frees_the_caches_and_spares_no_frame_shows() {
    let gpu = Gpu::headless();
    let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
    let target = Offscreen::new(&gpu.device, 64, 128);
    let mut atlas = crate::text::TextSystem::default().atlas;
    for instances in [rows(8), rows_and_menu(), rows(2)] {
        renderer.prepare(&gpu.device, &gpu.queue, &mut atlas, &instances, (64, 128));
    }
    assert_eq!((renderer.layers.retained.len(), renderer.layers.spare.len()), (6, 1), "six rows out of sight, and the menu's target");
    assert_eq!(atlas.resident_caches.len(), 8);
    let (shown, held) = (pixels(&gpu, &renderer, &target), renderer.layer_stats());
    assert!(pooled_bytes(&renderer) > 0);

    renderer.trim_memory(&mut atlas, &ImageAtlas::default());
    assert_eq!(pooled_bytes(&renderer), 0);
    assert!(renderer.layers.retained.is_empty() && renderer.layers.spare.is_empty());
    let mut left: Vec<u64> = atlas.resident_caches.keys().copied().collect();
    left.sort_unstable();
    assert_eq!(left, [0, 1], "the rows on screen keep their textures");
    assert_eq!(renderer.layer_stats().targets, 2);
    assert!(renderer.layer_stats().bytes < held.bytes);
    // The frame it prepared draws again as it was, and a second trim finds nothing.
    assert_eq!(pixels(&gpu, &renderer, &target), shown);
    let left = renderer.layer_stats();
    renderer.trim_memory(&mut atlas, &ImageAtlas::default());
    assert_eq!((renderer.layer_stats().targets, renderer.layer_stats().bytes), (left.targets, left.bytes));

    renderer.prepare(&gpu.device, &gpu.queue, &mut atlas, &rows(2), (64, 128));
    assert_eq!(renderer.layer_stats().allocations, held.allocations, "the frame on screen makes no texture");
    assert_eq!(pixels(&gpu, &renderer, &target), shown);

    let mut fresh = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
    let mut fresh_atlas = crate::text::TextSystem::default().atlas;
    for (renderer, atlas) in [(&mut renderer, &mut atlas), (&mut fresh, &mut fresh_atlas)] {
        renderer.prepare(&gpu.device, &gpu.queue, atlas, &rows_and_menu(), (64, 128));
        renderer.prepare(&gpu.device, &gpu.queue, atlas, &rows(8), (64, 128));
    }
    assert_eq!(pixels(&gpu, &renderer, &target), pixels(&gpu, &fresh, &target));
}

/// The backdrop copies stay while an effect on screen reads one, go with the blur's pyramid
/// once none does, and come back with the effect.
#[test]
fn a_trim_frees_the_backdrops_nothing_on_screen_reads() {
    let gpu = Gpu::headless();
    let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
    let target = Offscreen::new(&gpu.device, 32, 32);
    let mut atlas = crate::text::TextSystem::default().atlas;
    let mut blur = rect([8.0, 8.0, 16.0, 16.0], [0.0, 0.0, 0.0, 0.5]);
    blur.params = [6.0, KIND_BACKDROP_BLUR, 0.0, 1.0];
    let frame = [rect([0.0, 0.0, 16.0, 32.0], [1.0, 0.0, 0.0, 1.0]), blur];
    renderer.prepare(&gpu.device, &gpu.queue, &mut atlas, &frame, (32, 32));
    let blurred = pixels(&gpu, &renderer, &target);
    assert!(!renderer.backdrops.is_empty() && renderer.backdrop_blur.is_some());

    renderer.trim_memory(&mut atlas, &ImageAtlas::default());
    assert!(!renderer.backdrops.is_empty() && renderer.backdrop_blur.is_some(), "the blur on screen keeps its backdrop");
    assert_eq!(pixels(&gpu, &renderer, &target), blurred);

    renderer.prepare(&gpu.device, &gpu.queue, &mut atlas, &frame[..1], (32, 32));
    assert!(!renderer.backdrops.is_empty(), "a backdrop is pooled past its effect");
    renderer.trim_memory(&mut atlas, &ImageAtlas::default());
    assert!(renderer.backdrops.is_empty());

    renderer.prepare(&gpu.device, &gpu.queue, &mut atlas, &frame, (32, 32));
    assert_eq!(pixels(&gpu, &renderer, &target), blurred);
}

/// A runtime and its renderer, driven in the order a `LayerHost` drives them: every frame is
/// built, uploaded, prepared, drawn and submitted.
struct Pair<S: 'static> {
    rt: Runtime<S>,
    renderer: Renderer,
    target: Offscreen,
    size: (u32, u32),
}

impl<S: 'static> Pair<S> {
    fn new(gpu: &Gpu, state: S, view: fn(&S, &mut Cx<S>) -> El, size: (u32, u32), scale: f32) -> Self {
        let points = (size.0 as f32 / scale, size.1 as f32 / scale);
        Self {
            rt: Runtime::new(state, view, points, Env::new(true, scale)),
            renderer: Renderer::new(&gpu.device, OFFSCREEN_FORMAT),
            target: Offscreen::new(&gpu.device, size.0, size.1),
            size,
        }
    }

    /// Builds a frame without presenting it.
    fn build(&mut self, dt: f64) { self.rt.frame(dt); }

    fn frame(&mut self, gpu: &Gpu, dt: f64) {
        self.rt.frame(dt);
        self.renderer.upload_images(&gpu.device, &gpu.queue, &mut self.rt.images);
        self.renderer.prepare(&gpu.device, &gpu.queue, &mut self.rt.text.atlas, &self.rt.instances, self.size);
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        self.renderer.draw(&mut encoder, &self.target.view, [0.0; 4]);
        self.renderer.submit(&gpu.queue, encoder);
    }

    fn frames(&mut self, gpu: &Gpu, count: usize) {
        for _ in 0..count { self.frame(gpu, 1.0 / 120.0); }
    }

    fn set(&mut self, gpu: &Gpu, state: S) {
        *self.rt.state = state;
        self.rt.invalidate();
        self.frame(gpu, 1.0 / 120.0);
    }

    fn trim(&mut self) {
        self.rt.trim_memory();
        self.renderer.trim_memory(&mut self.rt.text.atlas, &self.rt.images);
    }

    /// The frame the renderer last prepared, drawn again.
    fn pixels(&self, gpu: &Gpu) -> Vec<u8> { pixels(gpu, &self.renderer, &self.target) }
}

fn cached_list(_: &(), _: &mut Cx<()>) -> El {
    col().key("scroll").size(64.0, 64.0).scroll(ScrollSpec { stick_bottom: false }).children((0..12).map(|i| {
        let shade = 0.2 + i as f32 * 0.06;
        el().key(format!("row{i}")).size(64.0, 16.0).padding(3.0).fill(Color { r: shade, g: 0.3, b: 1.0 - shade, a: 1.0 })
            .child(el().size(20.0 + i as f32 * 3.0, 10.0).radius(3.0).fill(Color::WHITE))
            .cache()
    }))
}

/// A cached row scrolled out of sight keeps its texture for its return. A trim frees it, and
/// scrolled back the row draws as a runtime and renderer that never scrolled draw it.
#[test]
fn a_cached_row_scrolled_out_and_trimmed_draws_again_when_it_returns() {
    let gpu = Gpu::headless();
    let mut pair = Pair::new(&gpu, (), cached_list, (64, 64), 1.0);
    pair.frame(&gpu, 0.0);
    pair.frames(&gpu, 2);
    let top = pair.pixels(&gpu);
    for _ in 0..8 {
        pair.rt.wheel(32.0, 32.0, -40.0);
        pair.frame(&gpu, 1.0 / 120.0);
    }
    pair.frames(&gpu, 600);
    assert_ne!(pair.pixels(&gpu), top, "the list scrolled");
    assert!(!pair.renderer.layers.retained.is_empty(), "rows out of sight keep their textures");
    let (scrolled, resident) = (pair.pixels(&gpu), pair.rt.text.atlas.resident_caches.len());

    pair.trim();
    assert!(pair.renderer.layers.retained.is_empty());
    assert!(pair.rt.text.atlas.resident_caches.len() < resident);
    assert_eq!(pair.pixels(&gpu), scrolled, "the frame on screen draws again as it was");
    pair.frames(&gpu, 1);
    assert_eq!(pair.pixels(&gpu), scrolled);

    for _ in 0..8 {
        pair.rt.wheel(32.0, 32.0, 40.0);
        pair.frame(&gpu, 1.0 / 120.0);
    }
    pair.frames(&gpu, 600);
    let mut fresh = Pair::new(&gpu, (), cached_list, (64, 64), 1.0);
    fresh.frame(&gpu, 0.0);
    fresh.frames(&gpu, 2);
    assert_eq!(fresh.pixels(&gpu), top);
    assert_eq!(pair.pixels(&gpu), top);
}

/// A trim for a host that has the app's `App::on_rest`: the hook runs once, and the trim
/// frees what one without it frees, with the frame on screen good to draw again.
#[test]
fn a_trim_with_the_apps_rest_hook_runs_it_once_and_frees_what_a_trim_frees() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static RESTS: AtomicUsize = AtomicUsize::new(0);
    fn rest(_: &mut (), _: &mut crate::Tx) -> bool {
        RESTS.fetch_add(1, Ordering::Relaxed);
        false
    }
    let gpu = Gpu::headless();
    let scrolled = |gpu: &Gpu| {
        let mut pair = Pair::new(gpu, (), cached_list, (64, 64), 1.0);
        pair.frame(gpu, 0.0);
        for _ in 0..8 {
            pair.rt.wheel(32.0, 32.0, -40.0);
            pair.frame(gpu, 1.0 / 120.0);
        }
        pair.frames(gpu, 600);
        pair
    };
    let (mut plain, mut hooked) = (scrolled(&gpu), scrolled(&gpu));
    let before = hooked.pixels(&gpu);
    assert!(!hooked.renderer.layers.retained.is_empty(), "rows out of sight keep their textures");

    plain.trim();
    assert!(!hooked.rt.trim_memory_with(Some(rest), &mut hooked.renderer), "a hook that changed nothing leaves a frame no work");
    assert_eq!(RESTS.load(Ordering::Relaxed), 1);
    assert!(hooked.renderer.layers.retained.is_empty());
    assert_eq!(hooked.rt.text.atlas.resident_caches, plain.rt.text.atlas.resident_caches);
    assert_eq!(hooked.renderer.atlas_pages.len(), plain.renderer.atlas_pages.len());
    assert_eq!(hooked.rt.text.atlas.pages.len(), plain.rt.text.atlas.pages.len());
    assert!(!hooked.rt.stale(), "a hook that changed nothing asks for no frame");
    assert_eq!(hooked.pixels(&gpu), before, "the frame on screen draws again as it was");
    hooked.frames(&gpu, 1);
    assert_eq!(hooked.pixels(&gpu), before);
}

/// A trim's hook that changed what the view reads, or asked its `Tx` for a wake, leaves a
/// frame work to do, which the trim reports for its host to ask for one. A trim with no hook,
/// or whose hook did neither, reports none.
#[test]
fn a_trim_says_when_its_rest_hook_left_a_frame_work_to_do() {
    fn changed(_: &mut (), _: &mut crate::Tx) -> bool { true }
    fn kept(_: &mut (), _: &mut crate::Tx) -> bool { false }
    fn timed(_: &mut (), tx: &mut crate::Tx) -> bool {
        tx.wake_after(std::time::Duration::from_secs(1));
        false
    }
    let gpu = Gpu::headless();
    for (hook, work) in [(None, false), (Some(kept as fn(&mut (), &mut crate::Tx) -> bool), false), (Some(changed), true), (Some(timed), true)] {
        let mut pair = Pair::new(&gpu, (), cached_list, (64, 64), 1.0);
        pair.frame(&gpu, 0.0);
        pair.frames(&gpu, 2);
        assert!(!pair.rt.stale());
        assert_eq!(pair.rt.trim_memory_with(hook, &mut pair.renderer), work);
        // The frame it asked for leaves none to ask for.
        pair.frames(&gpu, 1);
        assert!(!pair.rt.trim_memory_with(None, &mut pair.renderer));
    }
}

/// A trim before the view's first frame, as a memory warning while the app is still starting:
/// the app has heard nothing of its view yet, and its hook does not run.
#[test]
fn a_trim_before_the_first_frame_does_not_run_the_apps_rest_hook() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static RESTS: AtomicUsize = AtomicUsize::new(0);
    fn rest(_: &mut (), _: &mut crate::Tx) -> bool {
        RESTS.fetch_add(1, Ordering::Relaxed);
        true
    }
    let gpu = Gpu::headless();
    let mut pair = Pair::new(&gpu, (), cached_list, (64, 64), 1.0);
    pair.rt.trim_memory_with(Some(rest), &mut pair.renderer);
    assert_eq!(RESTS.load(Ordering::Relaxed), 0);
    pair.frame(&gpu, 0.0);
    assert!(pair.rt.trim_memory_with(Some(rest), &mut pair.renderer));
    assert_eq!(RESTS.load(Ordering::Relaxed), 1);
}

const PROSE: &str = "Amber lantern orchard quiet harbor, copper meadow signal drift. Pebble willow \
    ember tide under a juniper kestrel; nimbus opal prism and a quiet harbor beneath the lantern. ";

/// A page of paragraphs, or one line once the reader has left it.
fn page(reading: &bool, _: &mut Cx<bool>) -> El {
    if !*reading {
        return col().size(160.0, 120.0).child(crate::text("Done").key("done").color(Color::WHITE));
    }
    col().size(160.0, 120.0).children((0..6).map(|i| {
        crate::text(format!("{i}. {PROSE}")).key(format!("p{i}"))
            .font(crate::TextStyle::new(11.0, 400.0)).color(Color::WHITE).w_full()
    }))
}

/// A trim while a page is leaving takes nothing its exit still draws: every frame after
/// matches a run that was never trimmed.
#[test]
fn a_trim_while_elements_leave_changes_no_frame_of_their_exit() {
    let gpu = Gpu::headless();
    let mut pairs = [Pair::new(&gpu, true, page, (320, 240), 2.0), Pair::new(&gpu, true, page, (320, 240), 2.0)];
    for pair in &mut pairs {
        pair.frame(&gpu, 0.0);
        pair.set(&gpu, false);
        pair.frames(&gpu, 2);
    }
    let [trimmed, plain] = &mut pairs;
    let leaving = trimmed.rt.instances.len();
    let _ = trimmed.trim();
    assert_eq!(trimmed.pixels(&gpu), plain.pixels(&gpu));
    for frame in 0..240 {
        trimmed.frame(&gpu, 1.0 / 120.0);
        plain.frame(&gpu, 1.0 / 120.0);
        assert_eq!(trimmed.rt.instances.len(), plain.rt.instances.len(), "frame {frame}");
        if frame % 8 == 0 { assert_eq!(trimmed.pixels(&gpu), plain.pixels(&gpu), "frame {frame}"); }
    }
    assert!(trimmed.rt.instances.len() < leaving, "the exit ended");
    assert_eq!(trimmed.pixels(&gpu), plain.pixels(&gpu));
}

/// A frame built and never presented, then a trim: the renderer last prepared an older frame
/// than the runtime built. The next frame draws as an untrimmed run's does.
#[test]
fn a_trim_after_a_frame_that_was_built_but_not_presented() {
    let gpu = Gpu::headless();
    let mut pairs = [Pair::new(&gpu, true, page, (320, 240), 2.0), Pair::new(&gpu, true, page, (320, 240), 2.0)];
    for pair in &mut pairs {
        pair.frame(&gpu, 0.0);
        *pair.rt.state = false;
        pair.rt.invalidate();
        pair.build(1.0 / 120.0);
        for _ in 0..240 { pair.build(1.0 / 120.0); }
    }
    let [trimmed, plain] = &mut pairs;
    trimmed.trim();
    for frame in 0..3 {
        trimmed.frame(&gpu, 1.0 / 120.0);
        plain.frame(&gpu, 1.0 / 120.0);
        assert_eq!(trimmed.pixels(&gpu), plain.pixels(&gpu), "frame {frame}");
    }
    trimmed.set(&gpu, true);
    plain.set(&gpu, true);
    trimmed.frames(&gpu, 240);
    plain.frames(&gpu, 240);
    assert_eq!(trimmed.pixels(&gpu), plain.pixels(&gpu));
}

type Pictures = (bool, Pixels, Pixels);

fn pictures((icon_shows, icon, banner): &Pictures, _: &mut Cx<Pictures>) -> El {
    let mut column = col().size(48.0, 48.0);
    if *icon_shows { column = column.child(el().key("icon").size(16.0, 16.0).pixels(icon)); }
    column.child(el().key("banner").size(41.0, 4.0).pixels(banner))
}

fn picture(shade: u8, width: u32, height: u32) -> Pixels {
    let pixels = Pixels::new();
    pixels.set(width, height, (0..width * height).flat_map(|i| [shade, (i % 251) as u8, 255 - shade, 255]).collect());
    pixels
}

/// An image page whose image left while another page still shows: the trim frees its pixels
/// and its texture at once, with no frame in between, and the image draws the same when it
/// shows again.
#[test]
fn a_trim_frees_an_image_pages_texture_while_another_page_shows() {
    let gpu = Gpu::headless();
    // The banner is too wide for a shelf page, so it has a page of its own after the icon's.
    let state = (true, picture(40, 16, 16), picture(200, crate::pixels::IMAGE_ATLAS_SIZE + 1, 4));
    let mut pair = Pair::new(&gpu, state.clone(), pictures, (48, 48), 1.0);
    pair.frame(&gpu, 0.0);
    pair.frames(&gpu, 2);
    let both = pair.pixels(&gpu);
    assert!(pair.renderer.images.len() == 2 && pair.renderer.images.iter().all(Option::is_some));

    pair.set(&gpu, (false, state.1.clone(), state.2.clone()));
    pair.frames(&gpu, 240);
    let banner = pair.pixels(&gpu);
    assert_ne!(banner, both);
    assert!(pair.renderer.images[0].is_some(), "the first page stays for the icon's return");

    pair.trim();
    assert!(pair.rt.images.pages[0].data.is_empty());
    assert!(pair.renderer.images[0].is_none() && pair.renderer.images[1].is_some());
    assert_eq!(pair.pixels(&gpu), banner, "the frame on screen draws again as it was");
    pair.frames(&gpu, 1);
    assert_eq!(pair.pixels(&gpu), banner);
    pair.trim();
    assert!(pair.renderer.images[0].is_none() && pair.renderer.images[1].is_some(), "nothing is left to free");
    assert_eq!(pair.pixels(&gpu), banner);

    pair.set(&gpu, state);
    pair.frames(&gpu, 240);
    assert!(pair.renderer.images[0].is_some());
    assert_eq!(pair.pixels(&gpu), both);
}

/// The glyph pages the atlas gives back lose their textures in the trim itself: an app in the
/// background draws no frame for an upload to do it.
#[test]
fn a_trim_frees_the_glyph_page_textures_with_no_frame() {
    const GLYPHS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let gpu = Gpu::headless();
    let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
    let mut text = crate::text::TextSystem::default();
    let paint = |text: &mut crate::text::TextSystem, size: f32| {
        let layout = text.layout_rich(GLYPHS, &[], crate::TextStyle::new(size, 400.0), 2.0, None);
        text.glyphs_in_band_cached(&layout, [0.0; 2], 1.0, false, f32::NEG_INFINITY..f32::INFINITY, &mut Vec::new());
    };
    // One frame of a small line and of sizes enough to spill past the first page.
    text.begin_frame();
    paint(&mut text, 12.0);
    let mut size = 40.0;
    while text.atlas.page_count() < 3 {
        paint(&mut text, size);
        size += 4.0;
        assert!(size < 400.0, "the sizes never filled two pages");
    }
    renderer.prepare(&gpu.device, &gpu.queue, &mut text.atlas, &[], (64, 64));
    assert_eq!(renderer.atlas_pages.len(), text.atlas.page_count() - 1);

    // The next frame draws the small line alone, from the first page.
    text.begin_frame();
    paint(&mut text, 12.0);
    renderer.prepare(&gpu.device, &gpu.queue, &mut text.atlas, &[], (64, 64));
    assert!(!renderer.atlas_pages.is_empty(), "the pages stay for the sizes' return");

    text.rest();
    text.trim_memory();
    renderer.trim_memory(&mut text.atlas, &ImageAtlas::default());
    assert_eq!(text.atlas.page_count(), 1);
    assert!(renderer.atlas_pages.is_empty());
}
