use accesskit::{Affine, Rect, TreeInfo};

use super::*;

fn node(role: Role, bounds: [f64; 4], children: &[u64]) -> Node {
    let mut n = Node::new(role);
    n.set_bounds(Rect { x0: bounds[0], y0: bounds[1], x1: bounds[0] + bounds[2], y1: bounds[1] + bounds[3] });
    n.set_children(children.iter().map(|id| NodeId(*id)).collect::<Vec<_>>());
    n
}

fn label(text: &str, bounds: [f64; 4]) -> Node {
    let mut n = node(Role::Label, bounds, &[]);
    n.set_value(text);
    n
}

fn whole(nodes: Vec<(u64, Node)>) -> TreeUpdate {
    TreeUpdate { nodes: nodes.into_iter().map(|(id, n)| (NodeId(id), n)).collect(), tree: Some(TreeInfo::new(NodeId(1))), tree_id: TreeId::ROOT, focus: NodeId(1) }
}

fn part(nodes: Vec<(u64, Node)>) -> TreeUpdate {
    TreeUpdate { tree: None, ..whole(nodes) }
}

fn pressable(mut n: Node) -> Node { n.add_action(Action::Click); n }

fn labels(mirror: &Mirror) -> Vec<&str> { mirror.elements().iter().map(|e| e.label.as_str()).collect() }

/// A window with a title, a button holding its own text, and a switch.
fn screen() -> Vec<(u64, Node)> {
    let mut heading = node(Role::Heading, [16.0, 60.0, 200.0, 30.0], &[]);
    heading.set_label("Messages");
    let mut switch = pressable(node(Role::Switch, [16.0, 200.0, 60.0, 30.0], &[]));
    switch.set_label("Mute");
    switch.set_toggled(Toggled::True);
    vec![
        (1, node(Role::Window, [0.0, 0.0, 400.0, 800.0], &[2, 3, 5])),
        (2, heading),
        (3, pressable(node(Role::Button, [300.0, 60.0, 80.0, 44.0], &[4]))),
        (4, label("Send", [10.0, 10.0, 60.0, 24.0])),
        (5, switch),
    ]
}

#[test]
fn a_tree_flattens_to_its_controls_and_text_in_order_with_containers_walked_through() {
    let mut mirror = Mirror::default();
    let changes = mirror.apply(whole(screen()));
    assert!(changes.layout);
    assert_eq!(labels(&mirror), ["Messages", "Send", "Mute"]);
    let [heading, button, switch] = mirror.elements() else { panic!() };
    assert_eq!(heading.traits, traits::STATIC_TEXT | traits::HEADER);
    assert_eq!((button.traits, button.rect), (traits::BUTTON, [300.0, 60.0, 80.0, 44.0]));
    assert_eq!((switch.traits, switch.value.as_deref()), (traits::BUTTON | traits::TOGGLE, Some("1")));
}

#[test]
fn text_inside_a_control_is_its_name_and_not_a_stop_of_its_own() {
    let mut mirror = Mirror::default();
    mirror.apply(whole(screen()));
    assert_eq!(mirror.elements().iter().filter(|e| e.label == "Send").count(), 1);
    assert_eq!(mirror.elements()[1].id, 3);
}

#[test]
fn an_offset_moves_everything_below_it_and_the_roots_scale_moves_nothing() {
    let mut root = node(Role::Window, [0.0, 0.0, 400.0, 800.0], &[2]);
    root.set_transform(Affine::scale(3.0));
    let mut list = node(Role::ScrollView, [0.0, 100.0, 400.0, 600.0], &[3]);
    list.set_transform(Affine::translate((0.0, -250.0)));
    let mut mirror = Mirror::default();
    mirror.apply(whole(vec![(1, root), (2, list), (3, label("Row", [16.0, 400.0, 200.0, 40.0]))]));
    assert_eq!(mirror.elements()[0].rect, [16.0, 150.0, 200.0, 40.0]);
}

#[test]
fn what_a_scroll_view_cuts_off_is_cut_from_the_frame_and_what_it_hides_collapses_onto_its_edge() {
    let mut list = node(Role::ScrollView, [0.0, 100.0, 400.0, 600.0], &[3, 4, 5]);
    list.set_clips_children();
    let mut mirror = Mirror::default();
    mirror.apply(whole(vec![
        (1, node(Role::Window, [0.0, 0.0, 400.0, 800.0], &[2])),
        (2, list),
        (3, label("Above", [16.0, 20.0, 200.0, 40.0])),
        (4, label("Half", [16.0, 80.0, 200.0, 40.0])),
        (5, label("Inside", [16.0, 300.0, 200.0, 40.0])),
    ]));
    assert_eq!(labels(&mirror), ["Above", "Half", "Inside"], "all three stay in the order");
    let rects: Vec<_> = mirror.elements().iter().map(|e| e.rect).collect();
    assert_eq!(rects, [[16.0, 100.0, 200.0, 0.0], [16.0, 100.0, 200.0, 20.0], [16.0, 300.0, 200.0, 40.0]]);
}

#[test]
fn hidden_nodes_and_everything_in_them_are_skipped_and_so_is_text_with_nothing_to_say() {
    let mut group = node(Role::Group, [0.0, 0.0, 100.0, 100.0], &[3]);
    group.set_hidden();
    let mut mirror = Mirror::default();
    mirror.apply(whole(vec![
        (1, node(Role::Window, [0.0, 0.0, 400.0, 800.0], &[2, 4, 5])),
        (2, group),
        (3, label("Unseen", [0.0, 0.0, 10.0, 10.0])),
        (4, label("  ", [0.0, 0.0, 10.0, 10.0])),
        (5, label("Seen", [0.0, 0.0, 10.0, 10.0])),
    ]));
    assert_eq!(labels(&mirror), ["Seen"]);
}

#[test]
fn an_empty_field_is_read_by_its_placeholder_and_a_filled_one_by_its_text() {
    let mut field = pressable(node(Role::TextInput, [0.0, 0.0, 200.0, 40.0], &[]));
    field.set_placeholder("Message");
    let mut mirror = Mirror::default();
    mirror.apply(whole(vec![(1, node(Role::Window, [0.0, 0.0, 400.0, 800.0], &[2])), (2, field.clone())]));
    assert_eq!((mirror.elements()[0].label.as_str(), mirror.elements()[0].value.as_deref()), ("Message", None));
    field.set_label("Draft");
    field.set_value("hello");
    let changes = mirror.apply(part(vec![(2, field)]));
    assert!(!changes.layout, "the same elements in the same order");
    assert_eq!((mirror.elements()[0].label.as_str(), mirror.elements()[0].value.as_deref()), ("Draft", Some("hello")));
}

#[test]
fn a_disabled_or_selected_control_says_so_and_a_slider_is_adjustable_with_its_value() {
    let mut tab = pressable(node(Role::Tab, [0.0, 0.0, 80.0, 40.0], &[]));
    tab.set_label("Inbox");
    tab.set_selected(true);
    tab.set_disabled();
    let mut slider = node(Role::Slider, [0.0, 50.0, 200.0, 40.0], &[]);
    slider.set_label("Volume");
    slider.set_numeric_value(40.0);
    slider.add_action(Action::Increment);
    let mut mirror = Mirror::default();
    mirror.apply(whole(vec![(1, node(Role::Window, [0.0, 0.0, 400.0, 800.0], &[2, 3])), (2, tab), (3, slider)]));
    assert_eq!(mirror.elements()[0].traits, traits::BUTTON | traits::DISABLED | traits::SELECTED);
    assert_eq!((mirror.elements()[1].traits, mirror.elements()[1].value.as_deref()), (traits::ADJUSTABLE, Some("40")));
    assert_eq!(mirror.request(3, Gesture::Increment).map(|r| r.action), Some(Action::Increment));
    assert_eq!(mirror.request(3, Gesture::Decrement), None, "the slider takes no decrement");
}

#[test]
fn a_double_tap_clicks_or_else_focuses_and_asks_nothing_of_what_takes_neither() {
    let mut field = node(Role::TextInput, [0.0, 0.0, 200.0, 40.0], &[]);
    field.add_action(Action::Focus);
    let mut mirror = Mirror::default();
    mirror.apply(whole(vec![
        (1, node(Role::Window, [0.0, 0.0, 400.0, 800.0], &[2, 3, 4])),
        (2, pressable(node(Role::Button, [0.0, 0.0, 10.0, 10.0], &[]))),
        (3, field),
        (4, label("Text", [0.0, 0.0, 10.0, 10.0])),
    ]));
    let mut action = |id| mirror.request(id, Gesture::Activate).map(|r| (r.action, r.target_node));
    assert_eq!(action(2), Some((Action::Click, NodeId(2))));
    assert_eq!(action(3), Some((Action::Focus, NodeId(3))));
    assert_eq!(action(4), None);
    assert_eq!(action(99), None);
}

#[test]
fn a_scroll_goes_to_the_nearest_scroll_view_around_the_element() {
    let mut list = node(Role::ScrollView, [0.0, 0.0, 400.0, 600.0], &[3]);
    list.add_action(Action::ScrollDown);
    list.add_action(Action::ScrollRight);
    list.set_scroll_y(0.0);
    list.set_scroll_y_max(400.0);
    list.set_scroll_x(0.0);
    list.set_scroll_x_max(200.0);
    let mut mirror = Mirror::default();
    mirror.apply(whole(vec![
        (1, node(Role::Window, [0.0, 0.0, 400.0, 800.0], &[2, 4])),
        (2, list),
        (3, label("Row", [0.0, 0.0, 10.0, 10.0])),
        (4, label("Outside", [0.0, 0.0, 10.0, 10.0])),
    ]));
    let mut scroll = |id, toward| mirror.request(id, Gesture::Scroll(toward)).map(|r| (r.action, r.target_node));
    assert_eq!(scroll(3, Toward::Down), Some((Action::ScrollDown, NodeId(2))));
    assert_eq!(scroll(3, Toward::Up), None, "the list is at its top");
    assert_eq!(scroll(3, Toward::Right), Some((Action::ScrollRight, NodeId(2))));
    assert_eq!(scroll(3, Toward::Left), None, "and at its left edge");
    assert_eq!(scroll(4, Toward::Down), None);
}

#[test]
fn the_readers_cursor_landing_on_an_element_asks_for_it_to_be_scrolled_into_view() {
    let mut row = label("Row", [0.0, 0.0, 10.0, 10.0]);
    row.add_action(Action::ScrollIntoView);
    let mut mirror = Mirror::default();
    mirror.apply(whole(vec![
        (1, node(Role::Window, [0.0, 0.0, 400.0, 800.0], &[2, 3])),
        (2, row),
        (3, label("Fixed", [0.0, 20.0, 10.0, 10.0])),
    ]));
    let mut landed = |id| mirror.request(id, Gesture::Focused).map(|r| (r.action, r.target_node));
    assert_eq!(landed(2), Some((Action::ScrollIntoView, NodeId(2))));
    assert_eq!(landed(3), None, "an element that takes no scroll is asked nothing");
}

#[test]
fn a_live_region_announces_new_words_once_and_nothing_it_held_before_a_reader_asked() {
    let live = |words: &str| {
        let mut n = node(Role::Status, [0.0, 0.0, 0.0, 0.0], &[]);
        n.set_label(words);
        n.set_live(Live::Polite);
        n
    };
    let mut mirror = Mirror::default();
    let first = mirror.apply(whole(vec![(1, node(Role::Window, [0.0, 0.0, 400.0, 800.0], &[2])), (2, live("Old news"))]));
    assert_eq!(first.announce, Vec::<String>::new());
    assert_eq!(mirror.apply(part(vec![(2, live("Sent"))])).announce, ["Sent"]);
    assert_eq!(mirror.apply(part(vec![(2, live("Sent"))])).announce, Vec::<String>::new());
    assert_eq!(mirror.apply(part(vec![(2, live("Sent\u{200B}"))])).announce, ["Sent\u{200B}"], "said again");
}

#[test]
fn nodes_the_tree_no_longer_reaches_leave_the_mirror() {
    let mut mirror = Mirror::default();
    mirror.apply(whole(screen()));
    let changes = mirror.apply(part(vec![(1, node(Role::Window, [0.0, 0.0, 400.0, 800.0], &[2]))]));
    assert!(changes.layout);
    assert_eq!(labels(&mirror), ["Messages"]);
    assert_eq!(mirror.request(3, Gesture::Activate), None);
}

#[test]
fn the_reader_follows_the_apps_focus_to_an_element_but_not_on_the_first_tree_or_to_a_container() {
    let mut mirror = Mirror::default();
    let focused = |focus| TreeUpdate { focus: NodeId(focus), ..part(vec![]) };
    assert_eq!(mirror.apply(TreeUpdate { focus: NodeId(3), ..whole(screen()) }).focus, None);
    assert_eq!(mirror.apply(focused(5)).focus, Some(5));
    assert_eq!(mirror.apply(focused(5)).focus, None, "it stayed");
    assert_eq!(mirror.apply(focused(1)).focus, None, "the window is no element");
    assert_eq!(mirror.apply(focused(3)).focus, Some(3));
}

fn scrolled(offset: f64, max: f64) -> Vec<(u64, Node)> {
    let mut list = node(Role::ScrollView, [0.0, 0.0, 400.0, 500.0], &[3]);
    for action in [Action::ScrollUp, Action::ScrollDown] { list.add_action(action); }
    list.set_scroll_y(offset);
    list.set_scroll_y_max(max);
    vec![(1, node(Role::Window, [0.0, 0.0, 400.0, 800.0], &[2])), (2, list), (3, label("Row", [0.0, 0.0, 10.0, 10.0]))]
}

#[test]
fn a_scroll_at_its_end_is_refused_and_one_that_moves_says_which_page_it_landed_on() {
    let mut mirror = Mirror::default();
    mirror.apply(whole(scrolled(0.0, 1000.0)));
    assert_eq!(mirror.request(3, Gesture::Scroll(Toward::Up)), None, "already at the top");
    assert_eq!(mirror.apply(part(vec![])).page, None, "a refused scroll says nothing");
    assert!(mirror.request(3, Gesture::Scroll(Toward::Down)).is_some());
    let [(_, _), (_, list), _] = &scrolled(450.0, 1000.0)[..] else { panic!() };
    assert_eq!(mirror.apply(part(vec![(2, list.clone())])).page.as_deref(), Some("Page 2 of 4"));
    assert_eq!(mirror.apply(part(vec![])).page, None, "said once");
    let [_, (_, list), _] = &scrolled(1000.0, 1000.0)[..] else { panic!() };
    mirror.apply(part(vec![(2, list.clone())]));
    assert_eq!(mirror.request(3, Gesture::Scroll(Toward::Down)), None, "already at the bottom");
    assert!(mirror.request(3, Gesture::Scroll(Toward::Up)).is_some());
    assert_eq!(mirror.apply(part(vec![])).page.as_deref(), Some("Page 4 of 4"));
}

#[test]
fn a_context_menu_is_a_custom_action_and_a_disclosure_says_whether_it_is_open() {
    let mut row = pressable(node(Role::Button, [0.0, 0.0, 200.0, 40.0], &[]));
    row.set_label("Details");
    row.set_expanded(false);
    row.add_action(Action::ShowContextMenu);
    let mut mirror = Mirror::default();
    mirror.apply(whole(vec![(1, node(Role::Window, [0.0, 0.0, 400.0, 800.0], &[2])), (2, row)]));
    assert_eq!((mirror.elements()[0].expanded, mirror.elements()[0].menu), (Some(false), true));
    assert_eq!(mirror.request(2, Gesture::Menu).map(|r| r.action), Some(Action::ShowContextMenu));
    mirror.apply(whole(screen()));
    assert_eq!((mirror.elements()[1].expanded, mirror.elements()[1].menu), (None, false));
    assert_eq!(mirror.request(3, Gesture::Menu), None);
}

#[test]
fn an_empty_field_named_by_its_placeholder_says_it_once() {
    let mut field = node(Role::TextInput, [0.0, 0.0, 200.0, 40.0], &[]);
    field.set_label("Search");
    field.set_placeholder("Search");
    let mut mirror = Mirror::default();
    mirror.apply(whole(vec![(1, node(Role::Window, [0.0, 0.0, 400.0, 800.0], &[2])), (2, field)]));
    assert_eq!((mirror.elements()[0].label.as_str(), mirror.elements()[0].value.as_deref()), ("Search", None));
}

fn fields() -> Vec<(u64, Node)> {
    let field = |role, label: &str, y| {
        let mut n = node(role, [0.0, y, 200.0, 40.0], &[]);
        n.set_label(label);
        n.add_action(Action::Focus);
        n
    };
    vec![
        (1, node(Role::Window, [0.0, 0.0, 400.0, 800.0], &[2, 3, 4, 5])),
        (2, field(Role::TextInput, "Name", 0.0)),
        (3, field(Role::MultilineTextInput, "Notes", 50.0)),
        (4, field(Role::PasswordInput, "Password", 100.0)),
        (5, field(Role::SearchInput, "Search", 150.0)),
    ]
}

#[test]
fn text_fields_say_they_are_text_fields_and_a_password_a_secure_one() {
    let mut mirror = Mirror::default();
    mirror.apply(whole(fields()));
    let kinds: Vec<_> = mirror.elements().iter().map(|e| traits::names(e.traits)).collect();
    assert_eq!(kinds, [vec!["text-field"], vec!["text-field"], vec!["secure-text-field"], vec!["search", "text-field"]]);
}

#[test]
fn only_the_focused_text_field_is_editing_and_it_stops_when_focus_moves_away() {
    let mut mirror = Mirror::default();
    let editing = |mirror: &Mirror| mirror.elements().iter().filter(|e| e.editing).map(|e| e.id).collect::<Vec<_>>();
    mirror.apply(TreeUpdate { focus: NodeId(2), ..whole(fields()) });
    assert_eq!(editing(&mirror), [2]);
    mirror.apply(TreeUpdate { focus: NodeId(3), ..part(vec![]) });
    assert_eq!(editing(&mirror), [3], "focus moved to another field mid-session");
    mirror.apply(TreeUpdate { focus: NodeId(1), ..part(vec![]) });
    assert_eq!(editing(&mirror), Vec::<u64>::new(), "focus left every field");
    mirror.apply(TreeUpdate { focus: NodeId(4), ..part(vec![]) });
    assert_eq!(editing(&mirror), Vec::<u64>::new(), "a password field never forwards its text");
}

#[test]
fn a_password_reads_as_its_dots_or_its_placeholder_and_never_its_text() {
    use crate::{Cx, Env, Runtime, TextEdit, col, text_input};
    struct Form { pw: TextEdit }
    let view = |s: &Form, _: &mut Cx<Form>| {
        col().child(text_input(&s.pw, crate::tok::type_::BODY).key("pw").single_line().label("Password")
            .placeholder("Enter a password", crate::Color::WHITE).w(200.0).on_edit(|s: &mut Form| &mut s.pw))
    };
    let mut rt = Runtime::new(Form { pw: TextEdit::new("hunter2").secure() }, view, (400.0, 200.0), Env::new(true, 1.0));
    let read = |rt: &mut Runtime<Form>| {
        rt.frame(0.0);
        let json = rt.ios_accessibility_json();
        assert!(!json.to_string().contains("hunter2"), "{json}");
        let pw = json.as_array().unwrap().iter().find(|e| e["key"] == "pw").cloned().unwrap();
        (pw["label"].as_str().map(str::to_owned), pw["value"].as_str().map(str::to_owned))
    };
    assert_eq!(read(&mut rt), (Some("Password".into()), Some("•••••••".into())));
    rt.update(|s, _| s.pw.set_revealed(true));
    assert_eq!(read(&mut rt), (Some("Password".into()), Some("•••••••".into())), "revealed on screen, still dots to a reader");
    rt.update(|s, _| s.pw = TextEdit::new("").secure());
    assert_eq!(read(&mut rt), (Some("Password".into()), Some("Enter a password".into())));
}

#[test]
fn typing_a_password_changes_its_dots_with_no_change_to_the_tree() {
    let mut mirror = Mirror::default();
    mirror.apply(whole(fields()));
    let value = |mirror: &Mirror| mirror.elements().iter().find(|e| e.id == 4).and_then(|e| e.value.clone());
    assert!(mirror.set_masks(vec![(4, "•••".into())]));
    mirror.refresh();
    assert_eq!(value(&mirror).as_deref(), Some("•••"));
    assert!(!mirror.set_masks(vec![(4, "•••".into())]), "the same dots are no change");
    assert!(mirror.set_masks(Vec::new()));
    mirror.refresh();
    assert_eq!(value(&mirror), None);
}

#[test]
fn a_first_ask_after_the_first_frame_reads_the_whole_screen_without_waiting_for_another() {
    use crate::{AccessibilityLink, Cx, Env, Runtime, col, row, text};
    let view = |_: &(), _: &mut Cx<()>| {
        col().child(text("Inbox").role("heading"))
            .child(row().key("send").role("button").focusable(true).on_press(|_: &mut ()| {}).child(text("Send")))
    };
    let mut rt = Runtime::new((), view, (400.0, 300.0), Env::new(true, 1.0));
    rt.frame(0.0);
    let mut link = AccessibilityLink::new();
    let core = rt.core_mut();
    link.activate();
    link.receive(core, "Mail");
    let mut mirror = Mirror::default();
    mirror.apply(link.update(core).expect("the ask reads the last frame"));
    assert_eq!(labels(&mirror), ["Inbox", "Send"]);
    rt.frame(0.0);
    let next = link.update(rt.core_mut()).map_or_else(Changes::default, |update| mirror.apply(update));
    assert!(!next.layout, "the frame after the answer moves nothing");
}
