//! How many dashes each side of a dashed border holds (`El::border_dash`). Every side holds a
//! whole number of periods, so its gaps all match, and a side keeps its count while it grows or
//! shrinks until its length is `HOLD` of a period past the next whole number. A box whose size
//! settles near a boundary between two counts doesn't flick between them.
use std::collections::HashMap;

use rune_motion::NodeId;

use crate::render::shader_consts::{DASH_COUNT_BASE, DASH_MIN_PERIOD, QUARTER_TURN};

const HOLD: f32 = 0.25;

const KEEP: u32 = 120;

const MOST: u32 = DASH_COUNT_BASE as u32 - 1;

struct Held {
    counts: [u32; 4],
    period: f32,
    seen: u32,
}

#[derive(Default)]
pub(in crate::runtime) struct DashCounts {
    held: HashMap<NodeId, Held>,
    frame: u32,
}

impl DashCounts {
    /// Called once before each paint pass.
    pub(in crate::runtime) fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        let frame = self.frame;
        if !self.held.is_empty() {
            self.held.retain(|_, h| frame.wrapping_sub(h.seen) < KEEP);
        }
    }

    /// Node `id`'s dash counts for a ring `bw` wide inside a `size` box with corner `radii`
    /// (device px) and a pattern `period` long, packed for the shader: top and right, then
    /// bottom and left.
    pub(in crate::runtime) fn packed(&mut self, id: NodeId, size: [f32; 2], radii: [f32; 4], bw: f32, period: f32) -> [f32; 2] {
        let lens = side_lengths(size, radii, bw);
        let period = period.max(DASH_MIN_PERIOD);
        let before = self.held.get(&id).filter(|h| h.period == period).map(|h| h.counts);
        let counts: [u32; 4] = std::array::from_fn(|k| hold(lens[k] / period, before.map(|b| b[k])));
        self.held.insert(id, Held { counts, period, seen: self.frame });
        let packed = |n: u32| if n > MOST { 0.0 } else { n as f32 };
        [packed(counts[0]) * DASH_COUNT_BASE + packed(counts[1]), packed(counts[2]) * DASH_COUNT_BASE + packed(counts[3])]
    }
}

fn hold(periods: f32, before: Option<u32>) -> u32 {
    if !periods.is_finite() {
        return 1;
    }
    match before {
        Some(n) if (periods - n as f32).abs() < 0.5 + HOLD => n,
        _ => periods.round().max(1.0) as u32,
    }
}

fn side_lengths(size: [f32; 2], radii: [f32; 4], bw: f32) -> [f32; 4] {
    let h = bw * 0.5;
    let m = (size[0] * 0.5).min(size[1] * 0.5);
    let rc = radii.map(|r| (r.abs().min(m) - h).max(0.0));
    let arc = rc.map(|r| r * QUARTER_TURN);
    let flat = [
        (size[0] - bw - rc[0] - rc[1]).max(0.0),
        (size[1] - bw - rc[1] - rc[2]).max(0.0),
        (size[0] - bw - rc[2] - rc[3]).max(0.0),
        (size[1] - bw - rc[3] - rc[0]).max(0.0),
    ];
    std::array::from_fn(|k| flat[k] + (arc[k] + arc[(k + 1) % 4]) * 0.5)
}
