use super::{LineCap, dist, slice};

/// The most dashes one stroke is cut into; a pattern that would need more strokes solid.
const MAX_DASHES: f32 = 16384.0;

/// A dash pattern for [`Shape::stroke_dashed`](super::Shape::stroke_dashed): `dash` on, `gap`
/// off, in the shape's coordinates, starting at the first point.
///
/// ```
/// use rune_core::Color;
/// use rune_core::shape::{Dash, LineCap, Shape};
///
/// let dots = Dash::new(0.0, 4.0).cap(LineCap::Round);
/// let line = Shape::new().stroke_dashed(vec![[0.0, 2.0], [40.0, 2.0]], false, 2.0, dots, Color::WHITE);
/// assert!(!line.is_empty());
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dash {
    dash: f32,
    gap: f32,
    phase: f32,
    cap: LineCap,
}

impl Dash {
    /// `dash` on and `gap` off, with butt ends and no phase.
    pub fn new(dash: f32, gap: f32) -> Dash {
        Dash { dash, gap, phase: 0.0, cap: LineCap::Butt }
    }

    /// Starts the pattern `phase` into itself (SwiftUI's `dashPhase`), so growing it moves the
    /// dashes back along the stroke.
    pub fn phase(self, phase: f32) -> Dash {
        Dash { phase: if phase.is_finite() { phase } else { 0.0 }, ..self }
    }

    /// How each dash ends: butt (the default) at its length, or round, a half-disc as wide as the
    /// stroke past each end, so a zero `dash` draws dots.
    pub fn cap(self, cap: LineCap) -> Dash {
        Dash { cap, ..self }
    }

    pub(super) fn line_cap(&self) -> LineCap {
        self.cap
    }

    /// The dashes along `points` (back to the first point when `closed`), or `None` when the
    /// pattern draws the stroke solid: a zero gap, a zero dash with butt ends, or more dashes
    /// than [`MAX_DASHES`].
    pub(super) fn pieces(&self, points: &[[f32; 2]], closed: bool) -> Option<Vec<Vec<[f32; 2]>>> {
        let valid = self.dash.is_finite() && self.gap.is_finite() && self.dash >= 0.0 && self.gap > 0.0;
        if !valid || (self.dash == 0.0 && self.cap == LineCap::Butt) {
            return None;
        }
        let mut ring = points.to_vec();
        if closed && points.len() > 2 {
            ring.push(points[0]);
        }
        let total: f32 = ring.windows(2).map(|w| dist(w[0], w[1])).sum();
        let period = self.dash + self.gap;
        if total / period > MAX_DASHES {
            return None;
        }
        let mut out: Vec<Vec<[f32; 2]>> = Vec::new();
        let first = -self.phase.rem_euclid(period);
        let mut start = first;
        while start < total || (!closed && start <= total) {
            let (from, to) = (start.max(0.0), (start + self.dash).min(total));
            if to > from || (self.dash == 0.0 && start >= 0.0) {
                let piece = slice(&ring, from, to);
                if piece.len() >= 2 {
                    out.push(piece);
                }
            }
            start += period;
        }
        let wraps = closed && out.len() > 1 && start - period + self.dash > total && first + self.dash > 0.0;
        if wraps {
            let head = out.remove(0);
            if let Some(last) = out.last_mut() {
                last.extend_from_slice(&head[1..]);
            }
        }
        Some(out)
    }
}
