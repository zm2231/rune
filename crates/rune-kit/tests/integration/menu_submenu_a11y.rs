use rune::accesskit::Action;
use rune::{Cx, El, Env, Runtime, col, row};
use rune_kit::{Button, Kit, Menu, MenuEntry, MenuItem, root};

#[derive(Default)]
struct S {
    menu_open: bool,
    process_open: bool,
}

fn view(s: &S, _: &mut Cx<S>) -> El {
    let k = Kit::neutral(true);
    let mut layer = Vec::new();
    let sub = vec![
        MenuEntry::item(MenuItem::new("Restart").key("restart")),
        MenuEntry::item(MenuItem::new("Stop").key("stop")),
    ];
    let process = MenuItem::new("Process")
        .key("process")
        .submenu(sub, s.process_open)
        .on_submenu(|s: &mut S, open| s.process_open = open);
    let trigger = Menu::new("app")
        .trigger(Button::new("App").build(&k))
        .item(MenuItem::new("New").key("new"))
        .item(process)
        .open(s.menu_open)
        .on_close(|s: &mut S| s.menu_open = false)
        .build(&k, &mut layer);
    root(col().w_full().h_full().padding(16.0).child(row().w_full().child(trigger)), layer)
}

fn mount() -> Runtime<S> {
    let mut rt = Runtime::new(S { menu_open: true, ..S::default() }, view, (800.0, 480.0), Env::new(true, 1.0));
    settle(&mut rt);
    rt
}

fn settle(rt: &mut Runtime<S>) {
    for _ in 0..4 {
        rt.frame(1.0 / 60.0);
    }
}

fn focused(rt: &Runtime<S>) -> Option<String> {
    rt.elements().into_iter().find(|e| e.focused).and_then(|e| e.key)
}

fn supports(rt: &mut Runtime<S>, key: &str, action: Action) -> bool {
    let tree = rt.accessibility_tree();
    tree.nodes.iter().find(|(_, n)| n.author_id() == Some(key)).is_some_and(|(_, n)| n.supports_action(action))
}

#[test]
fn press_from_assistive_technology_opens_a_submenu_with_focus_inside() {
    let mut rt = mount();
    assert!(supports(&mut rt, "process", Action::Click), "a submenu row is pressable");
    assert!(rt.accessibility_action("process", Action::Click, None));
    settle(&mut rt);
    assert!(rt.state.process_open, "AXPress opens the submenu");
    assert_eq!(focused(&rt).as_deref(), Some("restart"), "focus moves to the submenu's first item");

    assert!(rt.accessibility_action("process", Action::Click, None));
    settle(&mut rt);
    assert!(rt.state.process_open, "pressing an open submenu row keeps it open");
    assert!(rt.state.menu_open, "and leaves the menu open");
}

#[test]
fn expand_and_collapse_open_and_close_a_submenu() {
    let mut rt = mount();
    assert!(supports(&mut rt, "process", Action::Expand));
    assert!(!rt.accessibility_action("process", Action::Collapse, None), "a closed submenu has nothing to collapse");
    assert!(rt.accessibility_action("process", Action::Expand, None));
    settle(&mut rt);
    assert!(rt.state.process_open);
    assert_eq!(focused(&rt).as_deref(), Some("restart"));

    assert!(supports(&mut rt, "process", Action::Collapse));
    assert!(rt.accessibility_action("process", Action::Collapse, None));
    settle(&mut rt);
    assert!(!rt.state.process_open, "Collapse closes the submenu");
    assert_eq!(focused(&rt).as_deref(), Some("process"), "focus returns to the submenu row");
    assert!(rt.state.menu_open, "the parent menu stays open");
}

#[test]
fn show_menu_opens_a_closed_submenu() {
    let mut rt = mount();
    assert!(supports(&mut rt, "process", Action::ShowContextMenu));
    assert!(!supports(&mut rt, "new", Action::ShowContextMenu), "a plain item has no menu to show");
    assert!(rt.accessibility_action("process", Action::ShowContextMenu, None));
    settle(&mut rt);
    assert!(rt.state.process_open, "AXShowMenu opens the submenu");
    assert_eq!(focused(&rt).as_deref(), Some("restart"));
}

#[test]
fn pointer_press_opens_a_submenu_without_moving_focus_into_it() {
    let mut rt = mount();
    let r = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("process")).expect("process row").target;
    let (x, y) = (r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(&mut rt);
    assert!(rt.state.process_open, "a click opens the submenu at once, as NSMenu does");
    assert_ne!(focused(&rt).as_deref(), Some("restart"), "the pointer keeps the highlight on the row");
}
