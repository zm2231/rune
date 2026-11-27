//! What a runtime gives back when the system is short of memory.

use super::*;

impl<S: 'static> Runtime<S> {
    /// For a host the system told it is short of memory (iOS's memory warning): gives back
    /// what a window at rest gives back, and then the text caches a rest keeps for a view
    /// shown again. The rest is the layouts the last two commits didn't use with their glyph
    /// runs, the images no element showed in the last frame with the CPU copies of the pages
    /// left empty, and the elements a rewind recording no longer refers to. On top of it go
    /// the layouts of the commit before the last that no element holds (a view just left), the
    /// glyph runs and colour glyphs the last frame laid no quads from and the glyph atlas
    /// pages it drew nothing from. All of it is made again from what the app still holds when
    /// it next shows, at the cost of shaping, rasterising and uploading it as on a first
    /// frame. Everything the app owns stays: an [`Image`](crate::Image) keeps its pixels
    /// while a handle or an element holds it.
    ///
    /// Nothing the last frame's instances draw from moves, so they stay good to draw again
    /// and no frame is asked for: the view isn't re-run. Call it between frames, never from a
    /// handler, and follow it with `Renderer::trim_memory` on this runtime's renderer, which
    /// lets go of the textures of what went; `LayerHost::trim_memory` does both.
    pub fn trim_memory(&mut self) {
        self.rest_with(None, None, false);
        self.text.trim_memory();
    }

    /// [`Self::trim_memory`] and `renderer`'s
    /// [`Renderer::trim_memory`](crate::render::Renderer::trim_memory) as one, for a host
    /// that has the app's `App::on_rest`: a rest of the two ([`Self::rest_with`] without `all`),
    /// which runs the hook where a window's rest does, and then what the two free on top of
    /// one. Like a rest it encodes, writes and submits nothing.
    ///
    /// True when a frame has work to do after it. The trim alone leaves none. A hook may:
    /// state the view reads changed, or it asked its `Tx` for what only a frame carries out
    /// or sends on (a scroll or text flight, a `wake_after`, a clipboard write). The host
    /// then asks for a frame if its view is on screen.
    #[cfg(any(test, target_vendor = "apple"))]
    pub(crate) fn trim_memory_with(&mut self, on_rest: Option<fn(&mut S, &mut Tx) -> bool>, renderer: &mut crate::render::Renderer) -> bool {
        self.rest_with(on_rest, Some(renderer), false);
        self.text.trim_memory();
        renderer.trim_pages(&self.text.atlas, &self.images);
        self.stale() || !self.frame.pending.wakes.is_empty() || self.copy_request.is_some() || self.edit.clear_secret_request
    }
}
