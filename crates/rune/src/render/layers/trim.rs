//! What the renderer gives back when the system is short of memory.
use super::*;

#[cfg(test)]
#[path = "trim_tests.rs"]
mod tests;

impl Renderer {
    /// After [`Runtime::trim_memory`](crate::Runtime::trim_memory) on the runtime whose glyph
    /// `atlas` (`rt.text.atlas`) and `images` (`rt.images`) these are: gives back what a
    /// window at rest gives back (`Renderer::give_back`: the [`El::cache`](crate::El::cache)
    /// textures no layer drew last frame, which `atlas` is told are gone, every spare layer
    /// target, and the backdrop copies and blur pyramid when the last frame read none), and
    /// then the textures of the glyph and image pages the runtime gave back, in the trim itself:
    /// an app in the background draws no frame for an upload to do it.
    ///
    /// It needs no device and draws nothing, so it is safe while the app is in the
    /// background. What the runtime's last frame draws with stays: when that frame is the one
    /// this renderer last prepared, [`Renderer::draw`] draws it again as it was.
    pub fn trim_memory(&mut self, atlas: &mut Atlas, images: &ImageAtlas) {
        self.give_back(false, &mut atlas.resident_caches, images);
        self.trim_pages(atlas, images);
    }

    /// What [`Self::trim_memory`] frees on top of a rest, for a caller that has rested: the
    /// textures of the pages the atlases dropped, and each live target's binding of them,
    /// which would keep a texture alive until the next frame binds again.
    pub(crate) fn trim_pages(&mut self, atlas: &Atlas, images: &ImageAtlas) {
        let glyph_pages = atlas.pages.len();
        self.atlas_pages.truncate(glyph_pages);
        let gone = |page: usize| images.pages.get(page).is_none_or(|p| p.data.is_empty());
        for (page, held) in self.images.iter_mut().enumerate() {
            if gone(page) { *held = None; }
        }
        self.images.truncate(images.pages.len());
        for t in self.layers.targets.iter_mut().flatten() {
            t.glyphs.truncate(glyph_pages);
            for (page, bind) in t.images.iter_mut().enumerate() {
                if gone(page) { *bind = None; }
            }
            t.images.truncate(images.pages.len());
        }
    }
}
