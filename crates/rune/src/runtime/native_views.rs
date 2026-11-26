use super::*;
use crate::native_view::handoff::{self, Effect, Event};
use crate::native_view::{NativeViewDraw, Request, Tag};
use crate::runtime::input::action::Action;
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
        for draw in &out {
            if let Some(slot) = crate::native_view::slot(draw.view) { slot.listen(self.frame.live.waker()); }
        }
        self.paint.native_views = out;
    }

    pub(in crate::runtime) fn take_native_view_reports(&mut self) {
        for draw in &self.paint.native_views {
            let Some(slot) = crate::native_view::slot(draw.view) else { continue };
            if slot.take_pressed() { self.frame.queue.push(Action::NativeView(Event::Pressed(draw.view))); }
            if let Some(back) = slot.take_tabbed() {
                self.frame.queue.push(Action::NativeView(Event::TabbedOut { view: draw.view, back }));
            }
            if slot.take_refused() { self.frame.queue.push(Action::NativeView(Event::Refused(draw.view))); }
        }
    }

    pub(in crate::runtime) fn native_view_event(&mut self, app: &mut dyn AppDriver, event: Event, tx: &mut Tx) -> bool {
        let mut ran = false;
        for effect in handoff::step(&mut self.kbd.hosted, event) {
            match effect {
                Effect::Enter { view, back } => ask(view, Request::Enter { back }),
                Effect::Blur => {
                    if self.kbd.focus.path().is_some() {
                        self.set_focus(app, None, self.kbd.focus.keyboard());
                        ran = true;
                    }
                }
                Effect::Continue { from, back } => ran |= self.tab_past_host(app, from, back, tx),
                Effect::Reclaim(view) => ask(view, Request::Reclaim),
            }
        }
        ran
    }

    pub(in crate::runtime) fn native_view_reclaim(&mut self) {
        crate::native_view::yield_to_rune();
        for effect in handoff::step(&mut self.kbd.hosted, Event::RuneFocused) {
            if let Effect::Reclaim(view) = effect { ask(view, Request::Reclaim); }
        }
    }

    pub(in crate::runtime) fn host_stops(&self) -> Vec<usize> {
        let mut seen = Vec::new();
        self.paint.native_views.iter()
            .filter(|d| { let first = !seen.contains(&d.view); seen.push(d.view); first })
            .filter(|d| crate::native_view::slot(d.view).is_some_and(|s| s.view.can_enter()))
            .filter_map(|d| self.tree.nodes.iter().position(|n| n.id == d.id))
            .filter(|&i| self.reachable(i))
            .collect()
    }

    pub(in crate::runtime) fn host_of(&self, view: crate::NativeViewId) -> Option<usize> {
        self.paint.native_views.iter().find(|d| d.view == view)
            .and_then(|d| self.tree.nodes.iter().position(|n| n.id == d.id))
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

fn ask(view: crate::NativeViewId, request: Request) {
    if let Some(slot) = crate::native_view::slot(view) { slot.ask(request); }
}
