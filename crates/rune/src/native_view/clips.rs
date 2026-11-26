use super::{NativeViewDraw, NativeViewId};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Placement {
    pub(crate) clip: [f64; 4],
    pub(crate) inner: [f64; 4],
    pub(crate) radius: f64,
}

impl Placement {
    pub(crate) fn of(draw: &NativeViewDraw, scale: f32) -> Self {
        let (frame, clip, radius) = draw.points(scale);
        Placement { clip, inner: [frame[0] - clip[0], frame[1] - clip[1], frame[2], frame[3]], radius }
    }
}

pub(crate) trait ClipView: Sized {
    type Root: ?Sized;
    type Container: ?Sized;
    fn make(root: &Self::Root) -> Option<Self>;
    fn holds(&self, container: &Self::Container) -> bool;
    fn adopt(&self, container: &Self::Container);
    fn place(&self, container: &Self::Container, at: &Placement);
    fn show(&self, alpha: f64, was_hidden: bool);
    fn hide(&self);
    fn raise(&self, root: &Self::Root);
    fn remove(&self);
}

struct Entry<V> {
    id: NativeViewId,
    view: V,
    placed: Option<Placement>,
    alpha: Option<f64>,
}

pub(crate) struct Clips<V> {
    entries: Vec<Entry<V>>,
    order: Vec<NativeViewId>,
}

impl<V> Default for Clips<V> {
    fn default() -> Self { Clips { entries: Vec::new(), order: Vec::new() } }
}

impl<V: ClipView> Clips<V> {
    pub(crate) fn idle(&self) -> bool { self.entries.iter().all(|e| e.alpha.is_none()) }

    pub(crate) fn prune(&mut self, alive: impl Fn(NativeViewId) -> bool) {
        self.entries.retain(|e| {
            let keep = alive(e.id);
            if !keep { e.view.remove(); }
            keep
        });
        self.order.retain(|id| alive(*id));
    }

    pub(crate) fn sync<'a>(&mut self, root: &V::Root, draws: &[NativeViewDraw], scale: f32, container: impl Fn(NativeViewId) -> Option<&'a V::Container>, adopted: impl Fn(NativeViewId))
    where
        V::Container: 'a,
    {
        let mut order = Vec::with_capacity(draws.len());
        for draw in draws {
            if order.contains(&draw.view) { continue; }
            let Some(held) = container(draw.view) else { continue };
            let k = match self.entries.iter().position(|e| e.id == draw.view) {
                Some(k) => k,
                None => {
                    let Some(view) = V::make(root) else { continue };
                    self.entries.push(Entry { id: draw.view, view, placed: None, alpha: None });
                    self.entries.len() - 1
                }
            };
            order.push(draw.view);
            let entry = &mut self.entries[k];
            if !entry.view.holds(held) {
                entry.view.adopt(held);
                entry.placed = None;
                adopted(draw.view);
            }
            let alpha = f64::from(draw.opacity);
            if entry.alpha != Some(alpha) { entry.view.show(alpha, entry.alpha.is_none()); }
            let at = Placement::of(draw, scale);
            if entry.placed != Some(at) { entry.view.place(held, &at); }
            entry.placed = Some(at);
            entry.alpha = Some(alpha);
        }
        for entry in self.entries.iter_mut().filter(|e| e.alpha.is_some() && !order.contains(&e.id)) {
            entry.view.hide();
            entry.alpha = None;
        }
        if order != self.order {
            for id in &order {
                if let Some(entry) = self.entries.iter().find(|e| e.id == *id) { entry.view.raise(root); }
            }
            self.order = order;
        }
    }

    pub(crate) fn views(&self) -> impl Iterator<Item = &V> { self.entries.iter().map(|e| &e.view) }
}
