use super::*;

pub(super) struct DotsBox {
    pub rect: [f32; 4],
    pub fill: Color,
    pub opacity: f32,
    pub clip: [f32; 4],
    pub fade: [f32; 4],
    pub stepped: bool,
    pub srgb: bool,
    pub wall: f64,
    pub reduced: bool,
}

pub(super) fn paint(instances: &mut Vec<Instance>, cycles: &mut keep::cycle::CyclePaint, i: usize, dots: &'static [rune_motion::CycleDot], b: DotsBox) {
    let [x, y, w, h] = b.rect;
    if w <= 0.0 || h <= 0.0 || b.fill.a * b.opacity <= 0.0005 {
        return;
    }
    let (srgb, wall) = (b.srgb, b.wall);
    let stepped = b.stepped && dots.iter().all(|d| d.cycle.stops.iter().all(|&(_, a)| a.is_finite() && a > 0.001 && a <= 1.0 && b.fill.a * a > 0.0005));
    if stepped {
        cycles.loop_recorded();
    }
    for d in dots {
        let a = if b.reduced { d.cycle.still } else { d.cycle.sample(wall) };
        let ink = blend::flat_base(b.fill, srgb, b.fill.a * b.opacity < 1.0 || d.cycle.stops.iter().any(|&(_, v)| v < 1.0));
        let [fx, fy, fw, fh] = d.rect;
        let rect = [x + fx * w, y + fy * h, fw * w, fh * h];
        let r = d.radius * rect[2].min(rect[3]);
        let total = b.opacity * a;
        if stepped {
            cycles.record_dot(i, instances.len(), d.cycle, (ink, b.opacity), rect);
        }
        instances.push(Instance {
            rect,
            radii: [r; 4],
            fill: ink.map(|v| v * total),
            border: [0.0; 4],
            params: [0.0, KIND_RECT, 0.0, total],
            uv: [0.0; 4],
            clip: b.clip,
            fade: crate::render::pack_fade(b.fade, b.clip, b.clip),
        });
    }
}
