use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;
use std::rc::Rc;

use rune::{Anim, Color, Curve, El, Live, ScrollPos, ScrollSource, Spring, Tx, el, icon, row};

use crate::Kit;

const SHOW_PAST: f32 = 1.0;
const WIDTH: f32 = 28.0;
const GLYPH: f32 = 16.0;
const FADE: Anim = Anim::new(Curve::Spring(Spring::new(0.15, 0.0)));
const KEPT: usize = 128;

struct Edges {
    pos: Cell<ScrollPos>,
    aim: Cell<Option<f32>>,
    shown: [Live<f32>; 2],
}

thread_local! {
    static EDGES: RefCell<HashMap<(u64, String), Rc<Edges>>> = RefCell::new(HashMap::new());
}

fn kept(path: &str) -> Rc<Edges> {
    let id = (rune::current_runtime_id(), path.to_owned());
    EDGES.with(|edges| {
        let mut edges = edges.borrow_mut();
        if edges.len() >= KEPT && !edges.contains_key(&id) {
            edges.retain(|_, e| Rc::strong_count(e) > 1);
            if edges.len() >= KEPT {
                edges.clear();
            }
        }
        edges.entry(id).or_insert_with(|| Rc::new(Edges { pos: Cell::new(ScrollPos::default()), aim: Cell::new(None), shown: [Live::new(0.0), Live::new(0.0)] })).clone()
    })
}

impl Edges {
    fn follow(&self, at: ScrollPos) {
        self.pos.set(at);
        if at.source != ScrollSource::Programmatic {
            self.aim.set(None);
        }
        let hidden = hidden(at);
        for (shown, hidden) in self.shown.iter().zip(hidden) {
            shown.set(if hidden > SHOW_PAST { 1.0 } else { 0.0 });
        }
    }

    fn page(&self, end: bool, chevron: f32, target: &str, tx: &mut Tx) {
        let at = self.pos.get();
        let page = (at.viewport[0] - 2.0 * chevron).max(at.viewport[0] / 2.0);
        let from = self.aim.get().unwrap_or(at.offset[0]);
        let to = (if end { from + page } else { from - page }).clamp(0.0, at.max[0].max(0.0));
        self.aim.set(Some(to));
        tx.live_only();
        tx.scroll_to(target, to, at.offset[1]);
    }
}

fn hidden(at: ScrollPos) -> [f32; 2] {
    let (offset, max) = (at.offset[0], at.max[0].max(0.0));
    [offset.clamp(0.0, max), (max - offset).clamp(0.0, max)]
}

/// Edge chevrons over a sideways scroll viewport ([`El::scroll_x`]): `ScrollOverflow::new("tabs",
/// row().scroll_x().scroll_edge_fade(24.0).child(list)).build(&k)`.
///
/// Each edge with content hidden past it shows a chevron on a backing in [`Self::fill`] (the
/// kit's `bg_base` by default) that fades into the content; pressing it pages the viewport toward
/// that edge, gliding on the scroll spring (a jump under reduced motion). Under the system's
/// Increase Contrast ([`Kit::increased_contrast`]) the backing is solid with a strong border on
/// its inner side. A chevron with nothing past its edge is hidden and takes no presses. The start
/// edge is the left one, or right to left ([`El::dir`]) the right one. The chevrons are pointer
/// affordances, neither focusable nor in the accessibility tree: keyboard focus and screen
/// readers reach what's past an edge themselves, and a focused element scrolls into view.
///
/// Size the element it returns as the viewport would be sized: the viewport shrinks to fit in
/// it and otherwise keeps its own size, so give it `.w_full()` to fill a wider wrapper. The
/// viewport is keyed `key`; the wrapper is keyed `{key}-overflow`.
pub struct ScrollOverflow {
    key: String,
    viewport: El,
    fill: Option<Color>,
}

impl ScrollOverflow {
    pub fn new(key: impl Into<String>, viewport: El) -> Self {
        ScrollOverflow { key: key.into(), viewport, fill: None }
    }

    /// The colour behind the chevrons: the surface the viewport sits on.
    pub fn fill(mut self, fill: Color) -> Self {
        self.fill = Some(fill);
        self
    }

    pub fn build(self, k: &Kit) -> El {
        let look = Look {
            fill: self.fill.unwrap_or(k.c.bg_base),
            border: k.c.border_strong,
            ink: if k.increased_contrast { k.c.text_strong } else { k.c.icon_base },
            hover: k.c.text_strong,
            solid: k.increased_contrast,
            width: k.text_size(WIDTH),
            glyph: k.text_size(GLYPH),
            chevron: k.glyph.chevron_down,
        };
        let key = self.key;
        let viewport = self.viewport.key(key.clone()).shrink().min_w(0.0).bind_path(|path, patch| {
            let area = path.rsplit_once('/').map_or("", |(p, _)| p);
            let edges = kept(area);
            patch.on_scroll_offset(move |at| edges.follow(at));
        });
        row().key(format!("{key}-overflow")).items_stretch().child(viewport).bind_path(move |area, patch| {
            let edges = kept(area);
            let target = rune::child_path(area, &key);
            for end in [false, true] {
                patch.child(chevron(&look, &edges, end, target.clone()));
            }
        })
    }
}

#[derive(Clone, Copy)]
struct Look {
    fill: Color,
    border: Color,
    ink: Color,
    hover: Color,
    solid: bool,
    width: f32,
    glyph: f32,
    chevron: &'static rune::Icon,
}

fn chevron(look: &Look, edges: &Rc<Edges>, end: bool, target: String) -> El {
    let (w, clear) = (look.width, look.fill.alpha(0.0));
    let mut backing = el()
        .key(if end { "overflow-end" } else { "overflow-start" })
        .pin((!end).then_some(0.0), Some(0.0), end.then_some(0.0), Some(0.0))
        .w(w)
        .center()
        .hover_group()
        .a11y_hidden()
        .focusable(false)
        .live_opacity(edges.shown[usize::from(end)].smooth_with(FADE));
    backing = if look.solid {
        let side = if end { [0.0, 0.0, 0.0, 1.0] } else { [0.0, 1.0, 0.0, 0.0] };
        backing.fill(look.fill).border_sides(side, look.border)
    } else {
        let angle = if end { -FRAC_PI_2 } else { FRAC_PI_2 };
        backing.linear_gradient(angle, [(0.0, look.fill), (0.6, look.fill), (1.0, clear)])
    };
    let glyph = icon(look.chevron, look.glyph).rotate(if end { -FRAC_PI_2 } else { FRAC_PI_2 }).mirror_in_rtl().color(look.ink).group_color(look.hover);
    let edges = edges.clone();
    backing.child(glyph).on_press_tx_any(move |tx| edges.page(end, w, &target, tx))
}

#[cfg(test)]
mod tests;
