//! An on-screen keyboard's height over the bottom of the window: what views read through
//! `Cx::keyboard_inset`, and the band a focused field's caret is kept inside.

use super::*;

impl window_requests::HostState {
    fn set_keyboard_inset(&mut self, bottom: f32) -> bool {
        let clean = if bottom.is_finite() { bottom.max(0.0) } else { 0.0 };
        if clean == self.keyboard_inset { return false; }
        self.keyboard_inset = clean;
        self.languages_gen = self.languages_gen.wrapping_add(1);
        true
    }

    /// The part of the window `(top, bottom)` in window points a person can see: above the
    /// keyboard, and inside a browser's visual viewport. `None` when all of it is.
    pub(in crate::runtime) fn visible_band(&self, height: f32) -> Option<(f32, f32)> {
        let keyboard = (self.keyboard_inset > 0.0).then(|| (0.0, (height - self.keyboard_inset).max(0.0)));
        match (self.visual_viewport_y, keyboard) {
            (Some((t0, b0)), Some((t1, b1))) => Some((t0.max(t1), b0.min(b1))),
            (band, None) | (None, band) => band,
        }
    }
}

impl RuntimeCore {
    pub(crate) fn set_keyboard_inset(&mut self, bottom: f32) {
        let before = self.host.keyboard_inset;
        if !self.host.set_keyboard_inset(bottom) { return; }
        self.commit.dirty = true;
        // A keyboard rising over a focused field keeps its caret in sight, as typing would.
        if self.host.keyboard_inset > before && self.focused_input().is_some() {
            self.edit.caret_follow = true;
        }
    }

    #[inline]
    pub(crate) fn keyboard_inset(&self) -> f32 {
        self.host.keyboard_inset
    }
}
