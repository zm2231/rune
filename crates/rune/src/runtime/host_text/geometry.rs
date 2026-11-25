//! Where the focused field's bytes are drawn, for a host's text system: line boxes, the caret,
//! and the byte under a point. All read the last frame's presented place (a field mid-spring or
//! scrolled is where it shows), in [`HostText::text`](super::HostText)'s offsets, which are the
//! drawn text's.

use std::ops::Range;

use super::*;

/// The focused field's laid text and where it draws: its text origin, scale and line height.
struct Drawn<'a> {
    flow: TextFlow<'a>,
    len: usize,
    origin: [f32; 2],
    zoom: f32,
    line: f32,
}

impl RuntimeCore {
    fn host_drawn(&self, key: &str) -> Option<(usize, Drawn<'_>)> {
        if self.kbd.focus.path() != Some(key) { return None; }
        let i = self.node_index(key)?;
        let n = &self.tree.nodes[i];
        let (inp, spec, laid) = (n.input.as_deref()?, n.text.as_deref()?, n.text_layout.as_ref()?);
        if !self.tree.built || self.paint.drawn.len() != self.tree.nodes.len() { return None; }
        let flow = laid.view();
        let s = self.env.scale;
        let (rect, _, _) = self.unturned_box_and_point(i, 0.0, 0.0);
        let zoom = self.paint.drawn_scale.get(i).copied().unwrap_or(1.0).max(1e-4);
        let [mut x, y] = links::origin(n, spec, flow, rect, s, zoom);
        if inp.single_line {
            x -= self.edit.single_line_scroll(&n.path) * zoom;
        }
        Some((i, Drawn { flow, len: spec.content.len(), origin: [x, y], zoom, line: spec.style.line_px() }))
    }

    /// The field's drawn box and the part of it shown, both `[x, y, w, h]`.
    pub(super) fn host_text_frame(&self, key: &str) -> Option<([f32; 4], [f32; 4])> {
        let i = self.node_index(key)?;
        let d = self.paint.drawn.get(i).copied()?;
        let edges = [d[0], d[1], d[0] + d[2], d[1] + d[3]];
        let clip = self.tree.nodes[i].parent.and_then(|p| self.paint.inh_scratch.get(p)).map_or(NO_CLIP, |h| h.clip).map(|v| v / self.env.scale);
        let shown = intersect(intersect(edges, clip), [0.0, 0.0, self.viewport.0, self.viewport.1]);
        let visible = [shown[0], shown[1], (shown[2] - shown[0]).max(0.0), (shown[3] - shown[1]).max(0.0)];
        Some((d, visible))
    }

    pub(crate) fn host_text_rects(&self, key: &str, range: Range<usize>) -> Vec<[f32; 4]> {
        let Some((_, d)) = self.host_drawn(key) else { return Vec::new() };
        let (a, b) = (range.start.min(d.len), range.end.min(d.len));
        if a >= b { return Vec::new(); }
        let s = self.env.scale;
        let k = d.zoom / s;
        d.flow.range_boxes(a, b, true).into_iter().map(|(bb, _)| [
            d.origin[0] + bb.x0 as f32 * k,
            d.origin[1] + bb.y0 as f32 * k,
            (bb.x1 - bb.x0) as f32 * k,
            (bb.y1 - bb.y0) as f32 * k,
        ]).collect()
    }

    pub(crate) fn host_text_caret(&self, key: &str, at: usize) -> Option<[f32; 4]> {
        let (_, d) = self.host_drawn(key)?;
        let s = self.env.scale;
        let k = d.zoom / s;
        let bb = d.flow.caret(at.min(d.len), parley::Affinity::Downstream, 2.0 * s);
        let h = if bb.y1 > bb.y0 { (bb.y1 - bb.y0) as f32 * k } else { d.line * d.zoom };
        Some([d.origin[0] + bb.x0 as f32 * k, d.origin[1] + bb.y0 as f32 * k, 2.0 * d.zoom, h])
    }

    pub(crate) fn host_text_offset_at(&self, key: &str, x: f32, y: f32) -> Option<usize> {
        if !x.is_finite() || !y.is_finite() { return None; }
        let (i, d) = self.host_drawn(key)?;
        let (_, ux, uy) = self.unturned_box_and_point(i, x, y);
        let s = self.env.scale;
        let (px, py) = ((ux - d.origin[0]) / d.zoom * s, (uy - d.origin[1]) / d.zoom * s);
        let h = (d.flow.height() - 1e-3).max(0.0);
        Some(d.flow.hit(px, py.clamp(0.0, h)).index.min(d.len))
    }
}
