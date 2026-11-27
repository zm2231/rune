use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use accesskit::{Action, ActionRequest, TreeId};

use super::app::{TypedWindowApp, WindowApp};
use crate::native_menu::{MenuGuard, fire};
use crate::{App, AppSpec, Cx, El, MenuItem, SharedState, col, el};

#[derive(Default)]
struct Board {
    presses: u32,
    bumps: u32,
}

fn board_view(_: &Board, _: &mut Cx<Board>) -> El {
    col().size(100.0, 100.0).child(
        el().key("press").role("button").label("Press").size(40.0, 20.0).on_press(|s: &mut Board| s.presses += 1),
    )
}

struct Window {
    app: TypedWindowApp<Board>,
    shortcuts: crate::shortcuts::Guard,
    menus: MenuGuard,
}

impl Window {
    fn new(shared: &SharedState<Board>, key: &str) -> Self {
        let mut app = App::new(AppSpec::new("board", (100.0, 100.0), Board::default, board_view))
            .menu_item(MenuItem::new("Bump", |s: &mut Board, _| s.bumps += 1));
        app.shared_state = Some(shared.clone());
        let shortcuts = crate::shortcuts::Guard::attach(&app.shared, Arc::new(|| {}), true, false);
        let menus = MenuGuard::attach(&app.shared, true);
        Self { app: TypedWindowApp::from_app(app, key, Arc::new(|| {})), shortcuts, menus }
    }

    fn turn<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        assert!(self.app.enter_turn());
        let result = f(self);
        self.app.leave_turn();
        result
    }

    fn frame(&mut self, dt: f64) { (&mut self.app as &mut dyn WindowApp).frame(dt); }

    fn drain(&mut self) { self.app.drain_window(&self.shortcuts, &mut self.menus); }

    fn press_request(&self) -> ActionRequest {
        let tree = self.app.rt().accessibility_tree();
        let (id, _) = tree.nodes.iter().find(|(_, n)| n.author_id() == Some("press")).expect("press node");
        ActionRequest { action: Action::Click, target_tree: TreeId::ROOT, target_node: *id, data: None }
    }
}

fn two_windows() -> (SharedState<Board>, Window, Window) {
    let shared = SharedState::new(Board::default());
    let (mut list, mut note) = (Window::new(&shared, "list"), Window::new(&shared, "note"));
    list.turn(|w| w.frame(0.0));
    note.turn(|w| w.frame(0.0));
    (shared, list, note)
}

#[test]
fn menu_and_accessibility_actions_reach_the_shared_state_through_a_window_not_in_its_turn() {
    let (_shared, mut list, mut note) = two_windows();
    fire(&note.app.shared, "Bump").unwrap();
    let press = note.press_request();
    assert!(note.app.rt_mut().a11y_action(&press));
    note.turn(|w| {
        w.drain();
        w.frame(0.016);
    });
    list.turn(|w| assert_eq!((w.app.rt.state.presses, w.app.rt.state.bumps), (1, 1)));
}

#[test]
#[should_panic(expected = "outside its turn")]
fn a_menu_drain_outside_the_windows_turn_is_caught() {
    let (_shared, _list, mut note) = two_windows();
    fire(&note.app.shared, "Bump").unwrap();
    note.drain();
}

#[test]
#[should_panic(expected = "outside its turn")]
fn a_frame_outside_the_windows_turn_is_caught() {
    let (_shared, _list, mut note) = two_windows();
    note.frame(0.016);
}

struct Tracked(Option<Rc<Cell<u32>>>);

impl Drop for Tracked {
    fn drop(&mut self) {
        if let Some(drops) = &self.0 { drops.set(drops.get() + 1); }
    }
}

#[test]
fn a_window_dropped_inside_its_turn_leaves_the_state_to_drop_once() {
    let drops = Rc::new(Cell::new(0));
    let shared = SharedState::with_placeholder(Tracked(Some(drops.clone())), || Tracked(None));
    let mut app = App::new(AppSpec::new("tracked", (10.0, 10.0), || Tracked(None), |_: &Tracked, _| el()));
    app.shared_state = Some(shared.clone());
    let mut window = TypedWindowApp::from_app(app, "only", Arc::new(|| {}));
    assert!(window.enter_turn());
    drop(window);
    assert_eq!(drops.get(), 0);
    drop(shared);
    assert_eq!(drops.get(), 1);
}
