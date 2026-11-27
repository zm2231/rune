//! Deliberately scalar transcription of shader.wgsl. No shared shader generator:
//! keeping the implementations independent catches changes to either lane.
use crate::{Atlas, Primitive};
use libm::{atan2f, cosf, expf, floorf, powf, roundf, sinf, sqrtf};

fn clamp(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}
fn mix(a: f32, b: f32, t: f32) -> f32 {
    a * (1.0 - t) + b * t
}
fn sd(p: [f32; 2], h: [f32; 2], radii: [f32; 4]) -> f32 {
    let index = if p[0] > 0.0 {
        if p[1] > 0.0 { 2 } else { 1 }
    } else if p[1] > 0.0 {
        3
    } else {
        0
    };
    if radii[index] < 0.0 {
        return sd_continuous(p, h, (-radii[index]).min(h[0].min(h[1])));
    }
    let r = radii[index].min(h[0].min(h[1]));
    let q = [p[0].abs() - h[0] + r, p[1].abs() - h[1] + r];
    let a = q[0].max(0.0);
    let b = q[1].max(0.0);
    q[0].max(q[1]).min(0.0) + sqrtf(a * a + b * b) - r
}
/// Apple's continuous corner: from (K·r, K·r) inside the corner, the curve lies K·r·g(u) away.
fn sd_continuous(p: [f32; 2], h: [f32; 2], r: f32) -> f32 {
    let e = 1.528_664_8 * r;
    let q = [h[0] - p[0].abs(), h[1] - p[1].abs()];
    let v = [e - q[0], e - q[1]];
    if v[0] <= 0.0 || v[1] <= 0.0 {
        return (-q[0]).max(-q[1]);
    }
    let u = atan2f(v[1], v[0]) * (4.0 / std::f32::consts::PI) - 1.0;
    let u2 = u * u;
    let g = 1.146_022_9 + u2 * (-0.375_731_6 + u2 * (0.307_966_1 - 0.077_243_54 * u2));
    sqrtf(v[0] * v[0] + v[1] * v[1]) - e * g
}
/// Matches WGSL `sign(r) * max(abs(r) - cut, 0.0)`: zero stays zero even when `cut < 0.0`.
fn inset_radius(r: f32, cut: f32) -> f32 {
    if r == 0.0 {
        0.0
    } else {
        (r.abs() - cut).max(0.0).copysign(r)
    }
}
fn ramp(d: f32, fade: f32) -> f32 {
    if fade == 0.0 {
        return clamp(d + 0.5);
    }
    let x = clamp(d / fade.abs());
    if fade < 0.0 { x } else { x * x * (3.0 - 2.0 * x) }
}
fn erf(x: f32) -> f32 {
    let s = if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    };
    let a = x.abs();
    let r = 1.0 + (0.278393 + (0.230389 + 0.078108 * (a * a)) * a) * a;
    let r = r * r;
    s - s / (r * r)
}
fn shadow(p: [f32; 2], h: [f32; 2], sigma: f32, radii: [f32; 4]) -> f32 {
    let lo = p[1] - h[1];
    let hi = p[1] + h[1];
    let start = (-3.0 * sigma).clamp(lo, hi);
    let end = (3.0 * sigma).clamp(lo, hi);
    let step = (end - start) / 4.0;
    let mut y = start + step * 0.5;
    let mut v = 0.0;
    for _ in 0..4 {
        let ry = p[1] - y;
        let [cl, cr] = if ry > 0.0 { [radii[3], radii[2]] } else { [radii[0], radii[1]] };
        let dl = (h[1] - cl - ry.abs()).min(0.0);
        let dr = (h[1] - cr - ry.abs()).min(0.0);
        let left_reach = h[0] - cl + sqrtf((cl * cl - dl * dl).max(0.0));
        let right_reach = h[0] - cr + sqrtf((cr * cr - dr * dr).max(0.0));
        let scale = sqrtf(0.5) / sigma;
        let left = 0.5 + 0.5 * erf((p[0] - right_reach) * scale);
        let right = 0.5 + 0.5 * erf((p[0] + left_reach) * scale);
        let gaussian =
            expf(-(y * y) / (2.0 * sigma * sigma)) / (sqrtf(2.0 * std::f32::consts::PI) * sigma);
        v += (right - left) * gaussian * step;
        y += step;
    }
    v
}

/// Coverage of a stroke segment from `a` to `b`, `hw` half width, round or butt caps
/// (`shader.wgsl` `segment`).
fn segment(p: [f32; 2], a: [f32; 2], b: [f32; 2], hw: f32, round: bool) -> f32 {
    let w = hw.max(0.5);
    let weight = (hw * 2.0).min(1.0);
    let ab = [b[0] - a[0], b[1] - a[1]];
    let len = sqrtf(ab[0] * ab[0] + ab[1] * ab[1]);
    let ap = [p[0] - a[0], p[1] - a[1]];
    let d = if len < 1e-4 {
        if !round {
            return 0.0;
        }
        sqrtf(ap[0] * ap[0] + ap[1] * ap[1]) - w
    } else {
        let dir = [ab[0] / len, ab[1] / len];
        let along = ap[0] * dir[0] + ap[1] * dir[1];
        if round {
            let t = along.clamp(0.0, len);
            let dx = ap[0] - dir[0] * t;
            let dy = ap[1] - dir[1] * t;
            sqrtf(dx * dx + dy * dy) - w
        } else {
            let across = (dir[0] * ap[1] - dir[1] * ap[0]).abs();
            let q = [(along - len * 0.5).abs() - len * 0.5, across - w];
            let mx = q[0].max(0.0);
            let my = q[1].max(0.0);
            q[0].max(q[1]).min(0.0) + sqrtf(mx * mx + my * my)
        }
    };
    clamp(0.5 - d) * weight
}

/// Signed distance from `p` to edge `a -> b`, positive on `c`'s side (`shader.wgsl` `edge`).
fn edge(p: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    let e = [b[0] - a[0], b[1] - a[1]];
    let l = sqrtf(e[0] * e[0] + e[1] * e[1]).max(1e-6);
    let n = [-e[1] / l, e[0] / l];
    let s = if (c[0] - a[0]) * n[0] + (c[1] - a[1]) * n[1] >= 0.0 {
        1.0
    } else {
        -1.0
    };
    ((p[0] - a[0]) * n[0] + (p[1] - a[1]) * n[1]) * s
}

/// Tie-breaking shared-edge ownership (`shader.wgsl` `owns`).
fn owns(p: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
    let swap = a[0] > b[0] || (a[0] == b[0] && a[1] > b[1]);
    let (lo, hi) = if swap { (b, a) } else { (a, b) };
    let e = [hi[0] - lo[0], hi[1] - lo[1]];
    let side = e[0] * (c[1] - lo[1]) - e[1] * (c[0] - lo[0]);
    let v = e[0] * (p[1] - lo[1]) - e[1] * (p[0] - lo[0]);
    if side > 0.0 { v >= 0.0 } else { v < 0.0 }
}

/// Antialiased coverage across a boundary edge `a -> b` (`shader.wgsl` `soft`).
fn soft(p: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    let d = edge(p, a, b, c);
    let e = [b[0] - a[0], b[1] - a[1]];
    let t = ((p[0] - a[0]) * e[0] + (p[1] - a[1]) * e[1]) / (e[0] * e[0] + e[1] * e[1]).max(1e-6);
    if !(0.0..=1.0).contains(&t) {
        return if d >= 0.0 { 1.0 } else { 0.0 };
    }
    clamp(d + 0.5)
}

/// Filled triangle coverage with per-edge antialiasing bitmask (`shader.wgsl` `triangle`).
fn triangle(p: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2], aa: u32) -> f32 {
    let mut cov = 1.0;
    if (aa & 1) == 1 {
        cov *= soft(p, a, b, c);
    } else if !owns(p, a, b, c) {
        return 0.0;
    }
    if (aa & 2) == 2 {
        cov *= soft(p, b, c, a);
    } else if !owns(p, b, c, a) {
        return 0.0;
    }
    if (aa & 4) == 4 {
        cov *= soft(p, c, a, b);
    } else if !owns(p, c, a, b) {
        return 0.0;
    }
    cov
}

struct RingSide {
    at: f32,
    len: f32,
    edge: f32,
    flat: [f32; 2],
    index: usize,
}

fn ring_arc(p: [f32; 2], origin: [f32; 2], size: [f32; 2], radii: [f32; 4], bw: f32) -> RingSide {
    let h = bw * 0.5;
    let m = (size[0] * 0.5).min(size[1] * 0.5);
    let rc = radii.map(|r| (r.abs().min(m) - h).max(0.0));
    let e = rc.map(|r| r + h);
    let arc = rc.map(|r| r * 1.5707964);
    let s_tr = (size[0] - bw - rc[0] - rc[1]).max(0.0);
    let s_right = s_tr + arc[1];
    let s_br = s_right + (size[1] - bw - rc[1] - rc[2]).max(0.0);
    let s_bottom = s_br + arc[2];
    let s_bl = s_bottom + (size[0] - bw - rc[2] - rc[3]).max(0.0);
    let s_left = s_bl + arc[3];
    let s_tl = s_left + (size[1] - bw - rc[3] - rc[0]).max(0.0);
    let top = p[0] - e[0];
    let right = s_right + p[1] - e[1];
    let bottom = s_bottom + size[0] - e[2] - p[0];
    let left = s_left + size[1] - e[3] - p[1];
    let (s, index, wrap) = if p[0] > size[0] * 0.5 {
        if p[1] > size[1] * 0.5 {
            let v = [p[0] - (size[0] - e[2]), p[1] - (size[1] - e[2])];
            if v[0] > 0.0 && v[1] > 0.0 {
                (s_br + atan2f(v[1], v[0]) * rc[2], if v[1] >= v[0] { 2 } else { 1 }, false)
            } else if size[0] - p[0] < size[1] - p[1] {
                (right, 1, false)
            } else {
                (bottom, 2, false)
            }
        } else {
            let v = [p[0] - (size[0] - e[1]), p[1] - e[1]];
            if v[0] > 0.0 && v[1] < 0.0 {
                (s_tr + atan2f(v[0], -v[1]) * rc[1], if v[0] >= -v[1] { 1 } else { 0 }, false)
            } else if size[0] - p[0] < p[1] {
                (right, 1, false)
            } else {
                (top, 0, false)
            }
        }
    } else if p[1] > size[1] * 0.5 {
        let v = [p[0] - e[3], p[1] - (size[1] - e[3])];
        if v[0] < 0.0 && v[1] > 0.0 {
            (s_bl + atan2f(-v[0], v[1]) * rc[3], if -v[0] >= v[1] { 3 } else { 2 }, false)
        } else if p[0] < size[1] - p[1] {
            (left, 3, false)
        } else {
            (bottom, 2, false)
        }
    } else {
        let v = [p[0] - e[0], p[1] - e[0]];
        if v[0] < 0.0 && v[1] < 0.0 {
            let wrap = -v[1] >= -v[0];
            (s_tl + atan2f(-v[1], -v[0]) * rc[0], if wrap { 0 } else { 3 }, wrap)
        } else if p[0] < p[1] {
            (left, 3, false)
        } else {
            (top, 0, false)
        }
    };
    let b = [-arc[0] * 0.5, s_tr + arc[1] * 0.5, s_br + arc[2] * 0.5, s_bl + arc[3] * 0.5, s_tl + arc[0] * 0.5];
    let flat = [s_tr, s_br - s_right, s_bl - s_bottom, s_tl - s_left];
    let edge = [
        arc[0] * 0.5 - origin[0] - e[0],
        arc[1] * 0.5 - origin[1] - e[1],
        arc[2] * 0.5 + origin[0] + size[0] - e[2],
        arc[3] * 0.5 + origin[1] + size[1] - e[3],
    ];
    let lo = if wrap { b[4] } else { b[index] };
    let start = arc[index] * 0.5;
    RingSide { at: s - lo, len: b[index + 1] - b[index], edge: edge[index], flat: [start, start + flat[index]], index }
}

fn dash_cov(side: &RingSide, d: [f32; 4], held: [f32; 2], ring: f32, n: f32, h: f32, soft: f32) -> f32 {
    let round_caps = d[1] < 0.0;
    let period = (d[0] + d[1].abs()).max(0.001);
    let pair = held[side.index / 2];
    let hi = floorf(pair / 4096.0);
    let mine = if side.index & 1 == 1 { pair - hi * 4096.0 } else { hi };
    let count = if mine >= 1.0 { mine } else { floorf(side.len / period + 0.4921875).max(1.0) };
    let step = (side.len / count).max(0.001);
    let phase = d[2] / period * step;
    let ends = d[0] * 0.5 + if round_caps { h } else { 0.0 };
    let j = floorf((side.at + phase) / step + 0.5);
    let mut m = f32::MAX;
    for k in [-1.0f32, 0.0, 1.0] {
        let c = (j + k) * step - phase;
        let t = side.edge + ends - c;
        let straight = clamp((c - side.flat[0]).min(side.flat[1] - c));
        m = m.min((side.at - c - (t - floorf(t + 0.4921875)) * straight).abs());
    }
    if round_caps {
        let along = (m - d[0] * 0.5).max(0.0);
        let reach = sqrtf(along * along + n * n);
        return ring.min(clamp(0.5 + (h - reach) / (soft * 2.0)));
    }
    ring * clamp(d[0] * 0.5 - m + 0.5)
}

/// A border over its own background, as CSS paints one (shader.wgsl `border_over`).
#[allow(clippy::too_many_arguments)]
fn border_over(fill: [f32; 4], edge: [f32; 4], d: f32, inner_d: f32, soft: f32, bw: f32, opacity: f32, shrink: bool) -> [f32; 4] {
    let cov = clamp(0.5 - d / (soft * 2.0));
    let ring = (cov - clamp(0.5 - inner_d / (soft * 2.0))).max(0.0);
    let hide = clamp(edge[3] / opacity.clamp(1e-6, 1.0));
    let pull = if shrink && hide > 0.999 { bw * 0.5 } else { 0.0 };
    let under = clamp(0.5 - (d + pull) / (soft * 2.0));
    std::array::from_fn(|c| edge[c] * ring + fill[c] * (under * (1.0 - hide * ring)))
}

/// How much of the pixel at `p` the box covers: the exact area at a square corner, the distance
/// estimate at a rounded one (shader.wgsl `box_cov`).
fn box_cov(p: [f32; 2], h: [f32; 2], radii: [f32; 4]) -> f32 {
    let r = if p[0] > 0.0 {
        if p[1] > 0.0 { radii[2] } else { radii[1] }
    } else if p[1] > 0.0 {
        radii[3]
    } else {
        radii[0]
    };
    if r == 0.0 {
        let e = |a: usize| clamp(h[a].min(p[a] + 0.5) - (-h[a]).max(p[a] - 0.5));
        return e(0) * e(1);
    }
    clamp(0.5 - sd(p, h, radii))
}

pub(crate) fn fragment(i: &Primitive, atlas: &Atlas, screen: [f32; 2]) -> [f32; 4] {
    let kind = i.params[1] as u32;
    // An effect slot (kind 12) holds a compositor's place in the stream and covers no pixels.
    if kind == 12 {
        return [0.0; 4];
    }
    // A glyph's params[0] turns its quad clockwise about the rect centre. Undo the turn so
    // bounds and sampling run in the glyph's own frame; the clip stays on screen.
    let rotated = kind == 2 && i.params[0] != 0.0;
    let p = if rotated {
        let (s, c) = (sinf(i.params[0]), cosf(i.params[0]));
        let center = [i.rect[0] + i.rect[2] * 0.5, i.rect[1] + i.rect[3] * 0.5];
        let d = [screen[0] - center[0], screen[1] - center[1]];
        [center[0] + (c * d[0] + s * d[1]), center[1] + (c * d[1] - s * d[0])]
    } else {
        screen
    };
    let grow = match kind {
        1 => i.params[2] * 3.0 + 1.0,
        2 => 0.0,
        3 | 5 | 6 => 1.0,
        _ => 1.0 + i.params[2] * 2.0,
    };
    let mut lo = [i.rect[0] - grow, i.rect[1] - grow];
    let mut hi = [lo[0] + (i.rect[2] + 2.0 * grow), lo[1] + (i.rect[3] + 2.0 * grow)];
    // One band of a big shadow or border ring rasterizes only its window (`shader.wgsl` `base_vs`).
    if kind <= 1 && i.uv[2] < 0.0 {
        lo = [lo[0].max(i.rect[0] + i.uv[0]), lo[1].max(i.rect[1] + i.uv[1])];
        hi = [hi[0].min(i.rect[0] - i.uv[2]).max(lo[0]), hi[1].min(i.rect[1] + i.uv[3]).max(lo[1])];
    }
    // Rasterization bounds matter for glyphs, bands and very soft shadows. Pixel centers,
    // half-open right/bottom edges, matching the two axis-aligned GPU triangles.
    if p[0] < lo[0] || p[1] < lo[1] || p[0] >= hi[0] || p[1] >= hi[1] {
        return [0.0; 4];
    }
    let h = [i.rect[2] * 0.5, i.rect[3] * 0.5];
    let q = [p[0] - (i.rect[0] + h[0]), p[1] - (i.rect[1] + h[1])];
    let mut col = [0.0; 4];
    match kind {
        0 => {
            let d = sd(q, h, i.radii);
            let soft = i.params[2].max(0.5);
            let bw = i.params[0];
            if bw > 0.0 {
                // Per-side widths (top, right, bottom, left) in `uv`; a band window has uv[2] < 0.
                let w = i.uv;
                let sides = w[2] >= 0.0 && w != [0.0; 4];
                let inner_d = if sides {
                    let ih = [(h[0] - (w[1] + w[3]) * 0.5).max(0.0), (h[1] - (w[0] + w[2]) * 0.5).max(0.0)];
                    let at = [q[0] - (w[3] - w[1]) * 0.5, q[1] - (w[0] - w[2]) * 0.5];
                    let cut = [w[0].max(w[3]), w[0].max(w[1]), w[2].max(w[1]), w[2].max(w[3])];
                    sd(at, ih, std::array::from_fn(|n| inset_radius(i.radii[n], cut[n])))
                } else {
                    d + bw
                };
                col = border_over(i.fill, i.border, d, inner_d, soft, bw, i.params[3], !sides && i.radii != [0.0; 4]);
            } else {
                col = i.fill.map(|c| c * clamp(0.5 - d / (soft * 2.0)));
            }
        }
        1 => {
            let m = h[0].min(h[1]);
            let raw = if i.uv[2] > 0.0 { [i.radii[0]; 4] } else { i.radii };
            let mut cov = shadow(q, h, i.params[2].max(0.01), raw.map(|r| r.abs().min(m)));
            if i.uv[2] > 0.0 {
                let hole = [i.uv[2] * 0.5, i.uv[3] * 0.5];
                let at = [p[0] - (i.rect[0] + i.uv[0] + hole[0]), p[1] - (i.rect[1] + i.uv[1] + hole[1])];
                cov *= clamp(0.5 + sd(at, hole, [i.radii[3]; 4]));
            }
            col = i.fill.map(|c| c * cov);
        }
        2 => {
            let cov = if i.uv[2] > 0.0 || rotated {
                let span = if i.uv[2] > 0.0 { [i.uv[2], i.uv[3]] } else { [i.rect[2], i.rect[3]] };
                let px = (p[0] - i.rect[0]) * span[0] / i.rect[2] - 0.5;
                let py = (p[1] - i.rect[1]) * span[1] / i.rect[3] - 0.5;
                let x = (i.uv[0] + floorf(px)) as i32;
                let y = (i.uv[1] + floorf(py)) as i32;
                let fx = px - floorf(px);
                let fy = py - floorf(py);
                mix(
                    mix(atlas.load(x, y), atlas.load(x + 1, y), fx),
                    mix(atlas.load(x, y + 1), atlas.load(x + 1, y + 1), fx),
                    fy,
                )
            } else {
                atlas.load(
                    (i.uv[0] + floorf(p[0] - i.rect[0])) as i32,
                    (i.uv[1] + floorf(p[1] - i.rect[1])) as i32,
                )
            };
            col = i.fill.map(|c| c * powf(cov, i.params[3]));
        }
        3 => {
            let cov = box_cov(q, h, i.radii);
            let spread = i.params[0];
            let ih = h.map(|v| (v - spread).max(0.0));
            let q = [q[0] - i.uv[0], q[1] - i.uv[1]];
            let lit = if i.params[2] < 0.3 {
                box_cov(q, ih, i.radii.map(|r| inset_radius(r, spread)))
            } else {
                let m = ih[0].min(ih[1]);
                shadow(
                    q,
                    ih,
                    i.params[2],
                    i.radii.map(|r| (r.abs() - spread).min(m).max(0.0)),
                )
            };
            col = i.fill.map(|c| c * cov * (1.0 - clamp(lit)));
        }
        4 => {
            let d = sd(q, h, i.radii);
            let soft = i.params[2].max(0.5);
            let bw = i.params[0];
            let t = clamp((p[1] - i.rect[1]) / i.rect[3].max(1.0));
            col = if bw > 0.0 {
                let edge = std::array::from_fn(|c| mix(i.border[c], i.uv[c], t));
                border_over(i.fill, edge, d, d + bw, soft, bw, i.params[3], i.radii != [0.0; 4])
            } else {
                let cov = clamp(0.5 - d / (soft * 2.0));
                std::array::from_fn(|c| mix(i.fill[c], i.uv[c], t) * cov)
            };
        }
        5 => {
            let cov = segment(
                screen,
                [i.radii[0], i.radii[1]],
                [i.radii[2], i.radii[3]],
                i.params[0],
                i.params[2] > 0.5,
            );
            col = i.fill.map(|c| c * cov);
        }
        6 => {
            let cov = triangle(
                screen,
                [i.radii[0], i.radii[1]],
                [i.radii[2], i.radii[3]],
                [i.uv[0], i.uv[1]],
                (i.params[0] + 0.5) as u32,
            );
            col = i.fill.map(|c| c * cov);
        }
        10 => {
            let d = sd(q, h, i.radii);
            let soft = i.params[2].max(0.5);
            let ring = clamp(0.5 - d / (soft * 2.0)) - clamp(0.5 - (d + i.params[0]) / (soft * 2.0));
            let t = clamp((p[1] - i.rect[1]) / i.rect[3].max(1.0));
            let local = [p[0] - i.rect[0], p[1] - i.rect[1]];
            let bw = i.params[0];
            let side = ring_arc(local, [i.rect[0], i.rect[1]], [i.rect[2], i.rect[3]], i.radii, bw);
            let cov = dash_cov(&side, i.uv, [i.params[3], i.uv[3]], ring, d + bw * 0.5, bw * 0.5, soft);
            col = std::array::from_fn(|c| mix(i.fill[c], i.border[c], t) * cov);
        }
        11 => {
            let d = sd(q, h, i.radii);
            let soft = i.params[2].max(0.5);
            let cov = clamp(0.5 - d / (soft * 2.0));
            let t0_raw = i.params[0];
            let t1_raw = i.params[3];
            let lower_open = t0_raw >= 1.5;
            let upper_open = t1_raw >= 1.5;
            let t0 = if lower_open { t0_raw - 2.0 } else { t0_raw };
            let t1 = if upper_open { t1_raw - 2.0 } else { t1_raw };
            let t = if i.uv[2] > 0.0 {
                let dx = (p[0] - i.rect[0] - i.uv[0]) / i.uv[2];
                let dy = (p[1] - i.rect[1] - i.uv[1]) / i.uv[3];
                sqrtf(dx * dx + dy * dy)
            } else {
                0.5 + (q[0] * i.uv[0] + q[1] * i.uv[1])
            };
            if !((lower_open && t < t0) || (upper_open && t >= t1)) {
                let u = if t1 <= t0 + 1e-6 {
                    if t >= t0 { 1.0 } else { 0.0 }
                } else {
                    clamp((t - t0) / (t1 - t0))
                };
                col = std::array::from_fn(|c| mix(i.fill[c], i.border[c], u) * cov);
            }
        }
        _ => panic!("unsupported primitive kind"),
    }
    let clip = clip_coverage(i, screen);
    col.map(|c| c * clip)
}

pub(crate) fn layer_clip_coverage(i: &Primitive, screen: [f32; 2]) -> f32 {
    clip_coverage(i, screen) * if i.params[3] == 0.0 { hole_alpha(screen, i.radii) } else { 1.0 }
}

/// A layer's round hole (`El::cutout_circle`): centre `h[0..2]`, clear within `h[2]`, whole from
/// `h[3]`, linear between and at least a pixel wide; none when `h[3]` is zero.
fn hole_alpha(p: [f32; 2], h: [f32; 4]) -> f32 {
    if h[3] <= 0.0 { return 1.0; }
    let (dx, dy) = (p[0] - h[0], p[1] - h[1]);
    clamp((sqrtf(dx * dx + dy * dy) - (h[2] + h[3]) * 0.5) / (h[3] - h[2]).max(1.0) + 0.5)
}

/// Matches `instance.rs::pack_clip_radii` so CPU clipping sees the same packed quantization
/// (`0.5` px / `2.0` px mixed steps, clamped to half the shorter clip side) as `clip.wgsl`.
pub(crate) fn pack_clip_radii(clip: [f32; 4], radii: [f32; 4]) -> u32 {
    let half = ((clip[2] - clip[0]).min(clip[3] - clip[1]) * 0.5).max(0.0);
    let clamped = radii.map(|radius| {
        let r = radius.abs().min(half).min(f32::from_bits(0x5fff_ffff));
        if radius < 0.0 && r > 0.0 { -r } else { r }
    });
    if clamped[0] == clamped[1] && clamped[1] == clamped[2] && clamped[2] == clamped[3] {
        return if clamped[0] == 0.0 { 0 } else { clamped[0].to_bits() };
    }
    let continuous = clamped.iter().any(|&r| r < 0.0);
    let max_r = clamped.iter().fold(0.0f32, |m, r| m.max(r.abs()));
    let coarse = max_r > 63.5;
    let inv_step = if coarse { 0.5 } else { 2.0 };
    let mut bits = 0x6000_0000u32
        | if continuous { 0x8000_0000 } else { 0 }
        | if coarse { 0x1000_0000 } else { 0 };
    for (idx, r) in clamped.into_iter().enumerate() {
        let q = roundf(r.abs() * inv_step).clamp(0.0, 127.0) as u32;
        bits |= q << (idx * 7);
    }
    bits
}

fn unpack_clip_radii(w: u32) -> [f32; 4] {
    if w == 0 {
        return [0.0; 4];
    }
    if (w & 0x6000_0000) != 0x6000_0000 {
        return [f32::from_bits(w); 4];
    }
    let step = if (w & 0x1000_0000) != 0 { 2.0 } else { 0.5 };
    let sign = if (w & 0x8000_0000) != 0 { -1.0 } else { 1.0 };
    std::array::from_fn(|idx| {
        let q = (w >> (idx * 7)) & 0x7f;
        if q == 0 { 0.0 } else { q as f32 * step * sign }
    })
}

fn clip_coverage(i: &Primitive, screen: [f32; 2]) -> f32 {
    let origin = i.fade_origin.unwrap_or(i.clip);
    let p = screen;
    let hard = [p[0] - i.clip[0], p[1] - i.clip[1], i.clip[2] - p[0], i.clip[3] - p[1]];
    let mut alpha: f32 = (0..4).map(|edge| {
        let width = half::f16::from_f32(i.fade[edge].clamp(-65504.0, 65504.0)).to_f32();
        let distance = if edge < 2 { i.clip[edge] - origin[edge] } else { origin[edge] - i.clip[edge] };
        let w = i.fade[edge].abs();
        let offset = if w > 0.0 { (distance / w).clamp(0.0, 1.0) } else { 0.0 };
        let offset = libm::floorf(offset * 255.0 + 0.5) / 255.0;
        if offset >= 1.0 { ramp(hard[edge], 0.0) }
        else if offset > 0.0 { ramp(hard[edge], 0.0).min(ramp(hard[edge] + offset * width.abs(), width)) }
        else { ramp(hard[edge], width) }
    }).product();
    let radii = if i.params[1] == 8.0 { i.clip_radii.unwrap_or(i.border) } else { i.clip_radii.unwrap_or([0.0; 4]) };
    let packed = pack_clip_radii(i.clip, radii);
    if packed != 0 {
        let half = [(i.clip[2] - i.clip[0]) * 0.5, (i.clip[3] - i.clip[1]) * 0.5];
        let q = [p[0] - i.clip[0] - half[0], p[1] - i.clip[1] - half[1]];
        alpha *= clamp(0.5 - sd(q, half, unpack_clip_radii(packed)));
    }
    alpha
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_negative_fade_ramps_linearly_and_a_positive_one_eases() {
        for (d, linear) in [(0.0, 0.0), (4.0, 0.25), (8.0, 0.5), (12.0, 0.75), (16.0, 1.0), (20.0, 1.0)] {
            assert!((ramp(d, -16.0) - linear).abs() < 1e-6, "linear at {d}: {}", ramp(d, -16.0));
        }
        assert!((ramp(4.0, 16.0) - 0.15625).abs() < 1e-6, "smoothstep a quarter in");
        assert_eq!(ramp(8.0, 16.0), ramp(8.0, -16.0), "both are half way at the middle");
    }

    #[test]
    fn ring_arc_walks_each_side_from_corner_middle_to_corner_middle() {
        for (size, radii, bw) in [
            ([40.0f32, 20.0], [10.0f32; 4], 2.0f32),
            ([30.0, 30.0], [0.0, 6.0, 15.0, 3.0], 1.0),
            ([50.0, 12.0], [6.0; 4], 0.5),
            ([24.0, 18.0], [0.0; 4], 1.5),
        ] {
            let h = bw * 0.5;
            let rc = radii.map(|r: f32| (r.min(size[0].min(size[1]) * 0.5) - h).max(0.0));
            let expected = 2.0 * (size[0] + size[1] - 2.0 * bw)
                - (2.0 - std::f32::consts::FRAC_PI_2) * rc.iter().sum::<f32>();
            let mut points = Vec::new();
            let steps = 400;
            for n in 0..steps {
                let angle = n as f32 / steps as f32 * std::f32::consts::TAU;
                let dir = [libm::sinf(angle), -libm::cosf(angle)];
                let (mut lo, mut hi) = (0.0, size[0] + size[1]);
                for _ in 0..60 {
                    let mid = (lo + hi) * 0.5;
                    let q = [dir[0] * mid, dir[1] * mid];
                    if sd(q, [size[0] * 0.5, size[1] * 0.5], radii) + h < 0.0 { lo = mid } else { hi = mid }
                }
                points.push([size[0] * 0.5 + dir[0] * lo, size[1] * 0.5 + dir[1] * lo]);
            }
            let mut trip = 0.0;
            let mut sides = 0.0;
            for n in 1..=steps {
                let (a, b) = (ring_arc(points[n - 1], [0.0; 2], size, radii, bw), ring_arc(points[n % steps], [0.0; 2], size, radii, bw));
                let ds = if b.index == a.index {
                    b.at - a.at
                } else {
                    sides += a.len;
                    assert_eq!(b.index, (a.index + 1) % 4, "{size:?} {radii:?} step {n}: sides run clockwise");
                    a.len - a.at + b.at
                };
                let (pa, pb) = (points[n - 1], points[n % steps]);
                let chord = sqrtf((pb[0] - pa[0]).powi(2) + (pb[1] - pa[1]).powi(2));
                assert!(ds > chord - 1e-3 && ds < chord * 1.5 + 1e-3, "{size:?} {radii:?} step {n}: {ds} vs {chord}");
                trip += ds;
            }
            assert!((trip - expected).abs() < 1e-2, "{trip} vs {expected}");
            assert!((sides - expected).abs() < 1e-2, "four sides add up to the line: {sides} vs {expected}");
        }
    }

    fn curved(at: f32, len: f32) -> RingSide {
        RingSide { at, len, edge: 0.0, flat: [1.0, 0.0], index: 0 }
    }

    fn gaps(len: f32, d: [f32; 4], held: [f32; 2]) -> Vec<f32> {
        let mut runs = Vec::new();
        let (mut run, steps) = (0.0, 4000);
        for k in 0..=steps {
            let at = len * k as f32 / steps as f32;
            if dash_cov(&curved(at, len), d, held, 1.0, 0.0, 1.0, 0.5) == 0.0 {
                run += len / steps as f32;
            } else if run > 0.0 {
                runs.push(run);
                run = 0.0;
            }
        }
        runs
    }

    #[test]
    fn every_side_holds_whole_periods_so_corners_take_dashes_and_gaps_match() {
        let butt = [4.0, 3.0, 0.0, 0.0];
        for len in [13.0, 37.3, 50.0, 251.9] {
            for d in [butt, [6.0, 2.5, 0.0, 0.0], [0.0, -3.0, 0.0, 0.0], [5.0, -4.0, 0.0, 0.0]] {
                assert_eq!(dash_cov(&curved(0.0, len), d, [0.0; 2], 1.0, 0.0, 1.0, 0.5), 1.0, "{len} {d:?}: a dash on the corner");
                assert_eq!(dash_cov(&curved(len, len), d, [0.0; 2], 1.0, 0.0, 1.0, 0.5), 1.0, "{len} {d:?}: and on the next");
                let runs = gaps(len, d, [0.0; 2]);
                let (lo, hi) = runs.iter().fold((f32::MAX, 0.0f32), |(lo, hi), g| (lo.min(*g), hi.max(*g)));
                assert!(hi - lo < len / 4000.0 * 1.5, "{len} {d:?}: gaps {runs:?}");
            }
        }
        let runs = gaps(70.0, butt, [10.0 * 4096.0, 0.0]);
        assert_eq!(runs.len(), 10, "a held count of 10 draws 10 gaps: {runs:?}");
        assert!(runs.iter().all(|g| (g - 2.0).abs() < 0.05), "the 3 gap, less a pixel of anti-aliasing: {runs:?}");
        let runs = gaps(70.0, butt, [8.0 * 4096.0, 0.0]);
        assert_eq!(runs.len(), 8, "and 8 stretches them: {runs:?}");
        assert!(runs.iter().all(|g| (g - (70.0 / 8.0 - 5.0)).abs() < 0.05), "{runs:?}");
        let quarter = [4.0, 4.0, 2.0, 0.0];
        assert_eq!(dash_cov(&curved(0.0, 48.0), quarter, [0.0; 2], 1.0, 0.0, 1.0, 0.5), 0.5, "phase moves each corner into the pattern");
        assert_eq!(dash_cov(&curved(48.0, 48.0), quarter, [0.0; 2], 1.0, 0.0, 1.0, 0.5), 0.5, "the same way at every corner");
        let round = [0.0, -4.0, 0.0, 0.0];
        assert_eq!(dash_cov(&curved(0.0, 40.0), round, [0.0; 2], 1.0, 0.0, 1.0, 0.5), 1.0, "a zero dash with round caps is a dot");
        assert_eq!(dash_cov(&curved(1.6, 40.0), round, [0.0; 2], 1.0, 0.0, 1.0, 0.5), 0.0, "one ring wide");
    }

    #[test]
    fn on_a_straight_run_dash_ends_land_where_the_ring_edges_do_on_the_pixel_grid() {
        let len = 75.0f32;
        for (d, h, edge) in [([0.0f32, -5.0, 0.0, 0.0], 1.5f32, 0.0f32), ([0.0, -5.0, 0.0, 0.0], 2.0, 0.3), ([6.0, 4.0, 0.0, 0.0], 1.0, 0.7)] {
            let ends = d[0] * 0.5 + if d[1] < 0.0 { h } else { 0.0 };
            let count = floorf(len / (d[0] + d[1].abs()) + 0.4921875);
            let side = |at: f32| RingSide { at, len, edge, flat: [0.0, len], index: 0 };
            let cov = |at: f32| dash_cov(&side(at), d, [0.0; 2], 1.0, 0.0, h, 0.5);
            for k in 2..count as usize - 1 {
                let ideal = k as f32 * len / count;
                let t = edge + ends - ideal;
                let centre = ideal + t - floorf(t + 0.4921875);
                assert!((ideal - centre).abs() <= 0.5);
                assert!((fract(centre - ends - edge)).min(1.0 - fract(centre - ends - edge)) < 1e-3, "{d:?} {k}: ends on the grid");
                for x in [0.25f32, 0.75, 1.25, 1.75] {
                    assert!((cov(centre - x) - cov(centre + x)).abs() < 1e-4, "{d:?} {k}: symmetric about {centre} at {x}");
                }
            }
        }
    }

    fn fract(v: f32) -> f32 {
        v - floorf(v)
    }

    #[test]
    fn pack_clip_radii_matches_uniform_fine_coarse_and_continuous_encoding() {
        let clip = [0.0, 0.0, 600.0, 600.0];
        assert_eq!(pack_clip_radii(clip, [0.0; 4]), 0);
        // Uniform corners round-trip as exact f32 bits.
        let uniform = pack_clip_radii(clip, [-12.25; 4]);
        assert_eq!(unpack_clip_radii(uniform), [-12.25; 4]);
        // Mixed corners <= 63.5 px quantize to 0.5 px steps.
        let fine = pack_clip_radii(clip, [10.2, 4.8, 0.0, 63.4]);
        assert_eq!(unpack_clip_radii(fine), [10.0, 5.0, 0.0, 63.5]);
        // Mixed corners > 63.5 px quantize to 2.0 px steps and saturate at 254 px.
        let coarse = pack_clip_radii(clip, [-71.3, -280.0, 0.0, -13.1]);
        assert_eq!(unpack_clip_radii(coarse), [-72.0, -254.0, 0.0, -14.0]);
        // Radii clamp to half the shorter clip side.
        let small_clip = [0.0, 0.0, 20.0, 40.0];
        assert_eq!(unpack_clip_radii(pack_clip_radii(small_clip, [50.0, 12.0, 15.0, 10.0])), [10.0; 4]);
    }

    #[test]
    fn inset_radius_keeps_zero_corners_square_under_negative_spread() {
        assert_eq!(inset_radius(0.0, -4.0), 0.0);
        assert_eq!(inset_radius(6.0, -4.0), 10.0);
        assert_eq!(inset_radius(-6.0, -4.0), -10.0);
        assert_eq!(inset_radius(-6.0, 8.0), -0.0);
    }
}
