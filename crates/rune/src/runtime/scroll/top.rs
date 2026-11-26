//! A tap on the status bar: the main list glides to its top, as `Tx::scroll_to(key, 0.0, 0.0)`
//! would send it. No key names "the main list", so it is found on screen: the first vertical
//! scroll viewport down the view's centre line from the status bar. A list under a title bar
//! is found below the bar, the column under the centre wins among columns side by side, a
//! pager's showing page wins among its pages, and a modal's own list wins over the one it
//! covers. Where viewports nest, the outermost one with anywhere to scroll goes.
//!
//! The first viewport down the line ends the search whether or not its content overflows: a
//! short conversation over a long composer scrolls nothing, where the composer's own text
//! would otherwise jump.
use super::*;

/// How far apart the points tried down the centre line are, in points: under the height of
/// any list worth scrolling.
const STEP: f32 = 8.0;

/// The points to try for a view `size` whose status bar takes `top` of it: down its centre
/// line, from just under the status bar to its bottom edge.
pub(super) fn line((width, height): (f32, f32), top: f32) -> impl Iterator<Item = (f32, f32)> {
    let first = top.max(0.0) + 1.0;
    let steps = if height > first { ((height - first) / STEP).ceil() as usize } else { 0 };
    (0..steps).map(move |k| (width / 2.0, first + k as f32 * STEP))
}

impl<S: 'static> Runtime<S> {
    /// Sends the main vertical viewport to its top: on the scroll spring, or at once under
    /// reduced motion. A hand, a fling or another request takes over mid-glide, and a
    /// `stick_bottom` viewport stops following its end. True when a viewport was sent, false
    /// when there is none or it is at its top already.
    pub fn scroll_main_to_top(&mut self) -> bool {
        let (core, mut app) = self.split_mut();
        core.scroll_main_to_top(&mut app)
    }

    pub fn main_scroll(&self) -> Option<[f32; 2]> {
        let core = self.core();
        let i = core.main_viewport()?;
        let st = core.scroll.scrolls.get(&core.tree.nodes[i].id)?;
        let zoom = core.zoom();
        Some([st.offset as f32 * zoom, st.max as f32 * zoom])
    }
}

impl RuntimeCore {
    /// [`Runtime::scroll_main_to_top`].
    pub(crate) fn scroll_main_to_top(&mut self, app: &mut dyn AppDriver) -> bool {
        // The content may have changed since the last frame measured it.
        self.step_scroll(0.0);
        let Some(i) = self.main_viewport() else { return false };
        if self.scroll.scrolls.get(&self.tree.nodes[i].id).is_none_or(|st| st.offset <= 0.5) { return false; }
        self.release_hover_hold();
        self.drop_scroll_holds();
        let glide = (!self.reduced_motion).then_some(crate::tok::motion::GLIDE);
        let mut jumps = Vec::new();
        self.send_viewport(i, (None, Some(0.0)), glide, crate::ScrollSource::Programmatic, &mut jumps);
        self.land_jumps(app, jumps, &[]);
        self.scroll.requests.step(requests::Event::Destination);
        true
    }

    /// The viewport a tap on the status bar scrolls: of the vertical viewports at the first
    /// point down the centre line that has one, the outermost whose content overflows.
    pub(super) fn main_viewport(&self) -> Option<usize> {
        let top = self.safe_area()[0] / self.zoom();
        let nodes = &self.tree.nodes;
        let vertical = |i: usize| nodes[i].scroll.is_some() && nodes[i].scroll_axes.y() && !self.subtree_exiting(i) && self.allowed(i);
        let innermost = line(self.viewport, top).find_map(|(x, y)| (0..nodes.len()).rev().find(|&i| vertical(i) && self.node_contains(i, x, y)))?;
        std::iter::successors(Some(innermost), |&i| nodes[i].parent)
            .filter(|&i| vertical(i) && self.scroll.scrolls.get(&nodes[i].id).is_some_and(|st| st.max > 0.5))
            .last()
    }
}

#[cfg(test)]
#[path = "top_tests.rs"]
mod tests;
