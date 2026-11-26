use super::*;
use crate::native_view::{NativeViewDraw, Tag};
use crate::runtime::turn::intersect;

impl RuntimeCore {
    pub(in crate::runtime) fn collect_native_views(&mut self) {
        let mut out = std::mem::take(&mut self.paint.native_views);
        out.clear();
        if crate::native_view::any_alive() {
            let n_nodes = self.tree.nodes.len();
            let order = &self.paint.layer_paint_order;
            for k in 0..if order.is_empty() { n_nodes } else { order.len() } {
                let i = if order.is_empty() { k } else { order[k] };
                if let Some(draw) = self.native_view_draw(i) { out.push(draw); }
            }
        }
        let mut pressed = false;
        for draw in &out {
            if let Some(slot) = crate::native_view::slot(draw.view) {
                slot.listen(self.frame.live.waker());
                pressed |= slot.take_pressed();
            }
        }
        self.paint.native_views = out;
        if pressed && self.kbd.focus.path().is_some() {
            self.frame.queue.push(crate::runtime::input::action::Action::Blur);
            self.frame.live.waker().notify();
        }
    }

    pub(crate) fn native_view_of(&self, i: usize) -> Option<crate::NativeViewId> {
        let Tag(view) = *self.tree.nodes.get(i)?.cold.get()?.data.iter().find_map(|d| d.downcast_ref::<Tag>())?;
        Some(view)
    }

    fn native_view_draw(&self, i: usize) -> Option<NativeViewDraw> {
        let n = self.tree.nodes.get(i)?;
        let view = self.native_view_of(i)?;
        if !self.paint.seen.get(i).copied().unwrap_or(false) || self.subtree_unmounted(i) { return None; }
        let opacity = self.paint.inh_scratch.get(i)?.opacity.clamp(0.0, 1.0);
        if opacity <= 0.001 { return None; }
        let s = self.env.scale;
        let [x, y, w, h] = self.paint.drawn.get(i)?.map(|v| v * s);
        if w <= 0.0 || h <= 0.0 { return None; }
        let window = [0.0, 0.0, self.viewport.0 * s, self.viewport.1 * s];
        let c = intersect(intersect(self.screen_clip_of_parent(n.parent), window), [x, y, x + w, y + h]);
        if c[2] <= c[0] || c[3] <= c[1] { return None; }
        let k = self.paint.drawn_scale.get(i).copied().unwrap_or(1.0);
        let radii = self.scene.presented(n.id).map_or([0.0; 4], |p| p.radius.0.map(|r| r * k * s));
        Some(NativeViewDraw {
            id: n.id,
            view,
            rect: [x, y, w, h],
            clip: [c[0], c[1], c[2] - c[0], c[3] - c[1]],
            radius: crate::system_material::clamp_radius(radii, w, h),
            opacity,
        })
    }
}
