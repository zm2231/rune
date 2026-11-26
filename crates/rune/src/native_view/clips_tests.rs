use std::cell::RefCell;

use super::clips::{ClipView, Clips, Placement};
use super::{NativeViewDraw, NativeViewId};
use rune_motion::NodeId;

#[derive(Debug, Clone, PartialEq)]
enum Op {
    Make(u32),
    Adopt(u32, u64),
    Place(u32, Placement),
    Show(u32, f64, bool),
    Hide(u32),
    Raise(u32),
    Remove(u32),
}

#[derive(Default)]
struct Window {
    ops: RefCell<Vec<Op>>,
    made: RefCell<u32>,
    holding: RefCell<Vec<(u32, u64)>>,
}

impl Window {
    fn take(&self) -> Vec<Op> { std::mem::take(&mut self.ops.borrow_mut()) }
}

struct Fake<'w> {
    n: u32,
    window: &'w Window,
}

impl<'w> ClipView for Fake<'w> {
    type Root = &'w Window;
    type Container = u64;

    fn make(root: &&'w Window) -> Option<Self> {
        let n = { let mut m = root.made.borrow_mut(); *m += 1; *m };
        root.ops.borrow_mut().push(Op::Make(n));
        Some(Fake { n, window: root })
    }
    fn holds(&self, container: &u64) -> bool { self.window.holding.borrow().contains(&(self.n, *container)) }
    fn adopt(&self, container: &u64) {
        let mut h = self.window.holding.borrow_mut();
        h.retain(|&(_, c)| c != *container);
        h.push((self.n, *container));
        self.window.ops.borrow_mut().push(Op::Adopt(self.n, *container));
    }
    fn place(&self, _: &u64, at: &Placement) { self.window.ops.borrow_mut().push(Op::Place(self.n, *at)); }
    fn show(&self, alpha: f64, was_hidden: bool) { self.window.ops.borrow_mut().push(Op::Show(self.n, alpha, was_hidden)); }
    fn hide(&self) { self.window.ops.borrow_mut().push(Op::Hide(self.n)); }
    fn raise(&self, _: &&'w Window) { self.window.ops.borrow_mut().push(Op::Raise(self.n)); }
    fn remove(&self) { self.window.ops.borrow_mut().push(Op::Remove(self.n)); }
}

fn draw(view: u64, y: f32, opacity: f32) -> NativeViewDraw {
    NativeViewDraw { id: NodeId(view), view: NativeViewId(view), rect: [0.0, y, 200.0, 100.0], clip: [0.0, y.max(0.0), 200.0, 100.0 - (-y).max(0.0)], radius: 0.0, opacity }
}

const CONTAINERS: [u64; 3] = [0, 100, 200];

fn sync<'w>(clips: &mut Clips<Fake<'w>>, window: &'w Window, draws: &[NativeViewDraw]) -> Vec<NativeViewId> {
    let adopted = RefCell::new(Vec::new());
    clips.sync(&window, draws, 1.0, |id| CONTAINERS.get(id.0 as usize), |id| adopted.borrow_mut().push(id));
    adopted.into_inner()
}

#[test]
fn a_first_show_makes_adopts_places_and_raises_then_a_still_frame_touches_nothing() {
    let window = Window::default();
    let mut clips = Clips::default();
    let adopted = sync(&mut clips, &window, &[draw(1, 10.0, 1.0)]);
    assert_eq!(adopted, [NativeViewId(1)], "the container joined a window, so its app is told");
    let placed = Placement { clip: [0.0, 10.0, 200.0, 100.0], inner: [0.0, 0.0, 200.0, 100.0], radius: 0.0 };
    assert_eq!(window.take(), [Op::Make(1), Op::Adopt(1, 100), Op::Show(1, 1.0, true), Op::Place(1, placed), Op::Raise(1)]);
    sync(&mut clips, &window, &[draw(1, 10.0, 1.0)]);
    assert_eq!(window.take(), []);
}

#[test]
fn scrolling_past_the_top_cuts_the_clip_and_shifts_the_container_up_inside_it() {
    let window = Window::default();
    let mut clips = Clips::default();
    sync(&mut clips, &window, &[draw(1, 10.0, 1.0)]);
    window.take();
    sync(&mut clips, &window, &[draw(1, -30.0, 1.0)]);
    let placed = Placement { clip: [0.0, 0.0, 200.0, 70.0], inner: [0.0, -30.0, 200.0, 100.0], radius: 0.0 };
    assert_eq!(window.take(), [Op::Place(1, placed)]);
}

#[test]
fn a_view_that_stops_showing_hides_and_shows_again_in_place() {
    let window = Window::default();
    let mut clips = Clips::default();
    sync(&mut clips, &window, &[draw(1, 0.0, 1.0)]);
    window.take();
    sync(&mut clips, &window, &[]);
    assert_eq!(window.take(), [Op::Hide(1)]);
    assert!(clips.idle());
    sync(&mut clips, &window, &[]);
    assert_eq!(window.take(), [], "hidden once");
    sync(&mut clips, &window, &[draw(1, 0.0, 0.5)]);
    assert_eq!(window.take(), [Op::Show(1, 0.5, true), Op::Raise(1)]);
}

#[test]
fn paint_order_changes_restack_the_views() {
    let window = Window::default();
    let mut clips = Clips::default();
    sync(&mut clips, &window, &[draw(1, 0.0, 1.0), draw(2, 0.0, 1.0)]);
    window.take();
    sync(&mut clips, &window, &[draw(2, 0.0, 1.0), draw(1, 0.0, 1.0)]);
    assert_eq!(window.take(), [Op::Raise(2), Op::Raise(1)]);
}

#[test]
fn a_dropped_slot_takes_its_clip_view_with_it() {
    let window = Window::default();
    let mut clips = Clips::default();
    sync(&mut clips, &window, &[draw(1, 0.0, 1.0), draw(2, 0.0, 1.0)]);
    window.take();
    clips.prune(|id| id != NativeViewId(1));
    assert_eq!(window.take(), [Op::Remove(1)]);
    assert_eq!(clips.views().count(), 1);
}

#[test]
fn a_container_another_window_took_is_adopted_back() {
    let window = Window::default();
    let mut clips = Clips::default();
    sync(&mut clips, &window, &[draw(1, 0.0, 1.0)]);
    window.take();
    window.holding.borrow_mut().clear();
    let adopted = sync(&mut clips, &window, &[draw(1, 0.0, 1.0)]);
    assert_eq!(adopted, [NativeViewId(1)]);
    let ops = window.take();
    assert_eq!(ops[0], Op::Adopt(1, 100));
    assert!(matches!(ops[1], Op::Place(1, _)), "placed again after the move: {ops:?}");
}
