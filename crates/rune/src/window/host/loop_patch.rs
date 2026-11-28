use crate::runtime::RuntimeCore;

#[derive(Clone, Copy, PartialEq)]
struct Shape {
    clear: [f32; 4],
    atlas_resets: u64,
    atlas_rasterized: u64,
}

impl Shape {
    fn of(rt: &RuntimeCore) -> Self {
        let atlas = &rt.text.atlas;
        Self { clear: rt.clear_color(), atlas_resets: atlas.resets(), atlas_rasterized: atlas.rasterized() }
    }
}

#[derive(Default)]
pub(in crate::window) struct LoopPatch {
    last: Option<Shape>,
}

impl LoopPatch {
    pub(in crate::window) fn check(&self, rt: &RuntimeCore) -> Result<(), &'static str> {
        let atlas = &rt.text.atlas;
        if atlas.dirty.is_some() || atlas.pages.iter().any(|p| p.dirty.is_some()) || atlas.has_pending_fills() {
            return Err("glyph atlas");
        }
        if rt.images.pages.iter().any(|p| p.dirty.is_some()) { return Err("image"); }
        match self.last {
            None => Err("first frame"),
            Some(last) if last.clear != rt.clear_color() => Err("clear"),
            Some(last) if last != Shape::of(rt) => Err("glyph atlas"),
            Some(_) => Ok(()),
        }
    }

    pub(in crate::window) fn drawn(&mut self, rt: &RuntimeCore) {
        self.last = Some(Shape::of(rt));
    }
}
