//! What a runtime and its renderer give back when their window or view rests.

use super::*;

impl RuntimeCore {
    /// The runtime's caches go back to what the screen on show uses: the text layouts the last
    /// two commits didn't use and their glyph runs, the images no element showed in the last
    /// frame and the CPU copies of the pages left empty, and the elements a rewind recording no
    /// longer refers to. A window's rest and an iOS view's both start here.
    pub(crate) fn rest_caches(&mut self) {
        self.text.rest();
        self.images.rest();
        if let Some(rewind) = self.rewind() { rewind.rest(); }
    }
}

impl<S: 'static> Runtime<S> {
    /// A rest, for a host whose view can't be seen or has drawn nothing for a while (an iOS
    /// view whose scene goes to the background): [`RuntimeCore::rest_caches`], then `on_rest`,
    /// the app's `App::on_rest`, for its own caches; a hook that says it changed state leaves
    /// the runtime's next frame to run the view again. An app whose view has not run yet hears
    /// no rest, as it hears no `App::on_visibility`: a view can hide before its first frame.
    /// Last, `renderer` gives back what it keeps between frames, and with `all` what the last
    /// frame used too ([`Renderer::give_back`](crate::render::Renderer::give_back)).
    ///
    /// It drops and forgets, and no more: nothing is encoded, written to a queue or submitted,
    /// so it is safe where the GPU takes no work, as in an iOS app in the background. What the
    /// GPU still holds for a submission goes when the host next polls its device.
    pub(crate) fn rest(&mut self, on_rest: Option<fn(&mut S, &mut Tx) -> bool>, renderer: Option<&mut crate::render::Renderer>, all: bool) {
        self.core_mut().rest_caches();
        if let Some(f) = on_rest && self.commit.commits > 0 { self.update_if(f); }
        if let Some(renderer) = renderer { renderer.give_back(all, &mut self.text.atlas.resident_caches); }
    }
}
