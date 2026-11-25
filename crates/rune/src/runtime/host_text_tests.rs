use super::*;
use crate::{Keyboard, ReturnKey, TextEdit, TextStyle, TouchPhase, col, el, text_input};

#[derive(Default)]
struct Form {
    name: TextEdit,
    email: TextEdit,
    note: TextEdit,
    submitted: usize,
    /// Pads the window's bottom by the keyboard, as a view with a bar pinned there does.
    pad: bool,
}

fn form(s: &Form, cx: &mut Cx<Form>) -> El {
    let style = TextStyle::new(16.0, 400.0);
    col().size(320.0, 480.0).child(
        col().key("scroller").w_full().grow(1.0).scroll(crate::ScrollSpec { stick_bottom: false }).gap(8.0)
            .child(text_input(&s.name, style).key("name").single_line().w_full().h(40.0)
                .return_key(ReturnKey::Next)
                .on_edit(|s: &mut Form| &mut s.name)
                .on_submit(|s: &mut Form, _| s.submitted += 1))
            .child(text_input(&s.email, style).key("email").single_line().w_full().h(40.0)
                .keyboard(Keyboard::Email)
                .on_edit(|s: &mut Form| &mut s.email))
            .child(el().w_full().h(280.0))
            .child(text_input(&s.note, style).key("note").w_full().h(80.0).on_edit(|s: &mut Form| &mut s.note))
            .child(el().w_full().h(400.0)),
    )
    .child(el().key("inset").w_full().h(if s.pad { cx.keyboard_inset() } else { 0.0 }))
}

fn rig() -> Runtime<Form> {
    let mut rt = Runtime::new(Form::default(), form, (320.0, 480.0), Env::new(true, 2.0));
    rt.frame(0.0);
    rt
}

fn tap(rt: &mut Runtime<Form>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

#[test]
fn nothing_editable_has_focus_until_a_field_is_tapped() {
    let mut rt = rig();
    assert_eq!(rt.host_text(), None);
    tap(&mut rt, "name");
    let t = rt.host_text().expect("the tapped field");
    assert!(t.key.ends_with("name"), "{}", t.key);
    assert!(t.single_line && !t.secure);
    assert_eq!(t.hints.return_key, ReturnKey::Next);
    assert!(t.hints.autocorrect);
    assert!(t.visible[2] > 0.0 && t.visible[3] > 0.0, "the field shows: {:?}", t.visible);
}

#[test]
fn an_email_field_asks_for_no_autocorrect() {
    let mut rt = rig();
    tap(&mut rt, "email");
    let t = rt.host_text().unwrap();
    assert_eq!(t.hints.keyboard, Keyboard::Email);
    assert!(!t.hints.autocorrect);
}

#[test]
fn a_host_edit_reaches_the_model_before_the_next_frame() {
    let mut rt = rig();
    tap(&mut rt, "name");
    let key = rt.host_text().unwrap().key;
    assert!(rt.host_text_edit(&key, HostEdit::Compose { text: "かな".into(), cursor: None }));
    let t = rt.host_text().unwrap();
    assert_eq!((t.text.as_str(), t.marked.clone()), ("かな", Some(0..6)));
    assert!(rt.host_text_edit(&key, HostEdit::Replace { range: 0..6, text: "仮名".into() }));
    assert_eq!(rt.host_text().unwrap().text, "仮名");
    rt.frame(0.0);
    assert_eq!(rt.state.name.text, "仮名");
    assert!(!rt.state.name.is_composing());
}

#[test]
fn an_edit_for_a_field_that_lost_focus_is_ignored() {
    let mut rt = rig();
    tap(&mut rt, "name");
    let stale = rt.host_text().unwrap().key;
    tap(&mut rt, "email");
    assert!(!rt.host_text_edit(&stale, HostEdit::Replace { range: 0..0, text: "x".into() }));
    rt.frame(0.0);
    assert_eq!((rt.state.name.text.as_str(), rt.state.email.text.as_str()), ("", ""));
    assert!(!rt.host_text_blur(&stale), "a stale blur leaves the new field focused");
}

#[test]
fn typed_text_and_return_go_through_the_keys() {
    let mut rt = rig();
    tap(&mut rt, "name");
    rt.host_text_type("hi");
    rt.frame(0.0);
    assert_eq!(rt.state.name.text, "hi");
    rt.host_text_type("\n");
    rt.frame(0.0);
    assert_eq!(rt.state.submitted, 1, "Return in a single-line field submits");
    assert_eq!(rt.state.name.text, "hi");
}

#[test]
fn the_host_blurs_the_field_when_its_keyboard_is_dismissed() {
    let mut rt = rig();
    tap(&mut rt, "name");
    let key = rt.host_text().unwrap().key;
    assert!(rt.host_text_blur(&key));
    rt.frame(0.0);
    assert_eq!(rt.host_text(), None);
}

#[test]
fn rects_and_points_agree_on_where_bytes_are_drawn() {
    let mut rt = rig();
    tap(&mut rt, "name");
    let key = rt.host_text().unwrap().key;
    rt.host_text_type("hello world");
    rt.frame(0.0);
    let t = rt.host_text().unwrap();
    let [fx, fy, fw, fh] = t.rect;
    let boxes = rt.host_text_rects(&key, 0..5);
    assert_eq!(boxes.len(), 1);
    let [x, y, w, h] = boxes[0];
    assert!(x >= fx && y >= fy && x + w <= fx + fw && y + h <= fy + fh + 0.5, "{:?} in {:?}", boxes[0], t.rect);
    let c0 = rt.host_text_caret(&key, 0).unwrap();
    let c5 = rt.host_text_caret(&key, 5).unwrap();
    assert!((c0[0] - x).abs() < 1.0 && (c5[0] - (x + w)).abs() < 1.0, "carets {c0:?} {c5:?} bound {:?}", boxes[0]);
    assert_eq!(rt.host_text_offset_at(&key, c5[0] + 0.5, c5[1] + c5[3] * 0.5), Some(5));
    assert!(rt.host_text_rects("elsewhere", 0..5).is_empty());
}

#[test]
fn a_view_reads_the_keyboard_inset() {
    let mut rt = rig();
    rt.state.pad = true;
    rt.set_keyboard_inset(300.0);
    for _ in 0..60 { rt.frame(0.016); }
    assert_eq!(rt.presented("inset").unwrap().0[3], 300.0);
    assert_eq!(rt.presented("scroller").unwrap().0[3], 180.0, "the rest of the window is above the keyboard");
    rt.set_keyboard_inset(f32::NAN);
    for _ in 0..60 { rt.frame(0.016); }
    assert_eq!(rt.presented("inset").unwrap().0[3], 0.0);
}

#[test]
fn a_rising_keyboard_lifts_the_focused_field_above_it() {
    let mut rt = rig();
    let top = |rt: &Runtime<Form>| rt.presented("note").unwrap().0;
    tap(&mut rt, "note");
    let before = top(&rt);
    assert!(before[1] + before[3] <= 480.5, "the note shows: {before:?}");
    rt.set_keyboard_inset(300.0);
    for _ in 0..30 { rt.frame(0.016); }
    let key = rt.host_text().unwrap().key;
    let caret = rt.host_text_caret(&key, 0).unwrap();
    assert!(caret[1] + caret[3] <= 180.5, "the caret {caret:?} sits above the keyboard's top at 180");
}

#[test]
fn a_long_press_in_the_focused_field_is_the_hosts_when_it_owns_selection() {
    for native in [false, true] {
        let mut rt = rig();
        rt.set_native_text_selection(native);
        tap(&mut rt, "name");
        let (x, y) = rt.find("name").unwrap();
        rt.touch(1, TouchPhase::Started, x, y, 0.0);
        rt.frame(0.6);
        rt.touch(1, TouchPhase::Ended, x, y, 0.6);
        rt.frame(0.0);
        assert_eq!(rt.popup_menu().is_some(), !native, "native selection {native}");
        assert!(rt.host_text().is_some(), "the field keeps focus");
    }
}
