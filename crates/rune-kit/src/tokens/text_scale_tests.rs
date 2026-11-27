use rune::{Cx, El, Env, Key, KeyEvent, Mods, Runtime, col, el, row};

use super::*;
use crate::{Badge, Button, Checkbox, Field, Keybind, KitCx, KitTheme, RadioItem, SegmentedControl, SidebarRow};

struct App {
    theme: KitTheme,
    scale: TextScale,
}

fn view(s: &App, cx: &mut Cx<App>) -> El {
    cx.with_text_scale(&s.scale, |cx| {
        cx.with_kit(&s.theme, |cx| {
            let k = cx.kit();
            col()
                .child(row().child(Button::new("Save").build(&k).key("button")))
                .child(el().key("box").size(120.0, 40.0).px(12.0))
        })
    })
}

fn sizes(scale: f32) -> ([f32; 4], [f32; 4]) {
    let app = App { theme: KitTheme::neutral(), scale: TextScale::new(scale) };
    let mut rt = Runtime::new(app, view, (400.0, 300.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.settle_all();
    rt.frame(0.0);
    (rt.presented("button").expect("button").0, rt.presented("box").expect("box").0)
}

#[test]
fn a_text_scale_grows_text_controls_and_leaves_the_rest_of_the_layout() {
    let (button, plain) = sizes(1.0);
    let (bigger, same) = sizes(1.5);
    assert_eq!(bigger[3], button[3] * 1.5, "the button's height holds its larger label");
    assert!(bigger[2] > button[2], "its label is wider");
    assert_eq!(same[2..], plain[2..], "a box with no text keeps its size");
}

#[test]
fn a_kit_scales_its_type_and_text_metrics_from_the_themes_own_size() {
    let k = Kit::neutral(true);
    let big = k.with_text_scale(1.5);
    assert_eq!(big.text_scale, 1.5);
    assert_eq!(big.ty.ui.size, k.ty.ui.size * 1.5);
    assert_eq!(big.ty.body.line_height, k.ty.body.line_height * 1.5);
    assert_eq!(big.radius.md, k.radius.md, "radii stay");
    let button = big.button_metrics(crate::ControlSize::Regular);
    let base = k.button_metrics(crate::ControlSize::Regular);
    assert_eq!((button.height, button.icon, button.padding_x), (base.height * 1.5, base.icon * 1.5, base.padding_x));
    assert_eq!(big.text_size(28.0), 42.0);
    assert_eq!(big.text(rune::TextStyle::new(12.0, 400.0)).size, 18.0);
    let back = big.with_text_scale(1.0);
    assert_eq!(back.ty.ui, k.ty.ui, "scales are from the theme's size, not compounded");
    assert_eq!(back.button_metrics(crate::ControlSize::Regular), base);
    assert_eq!(k.with_text_scale(f32::NAN).text_scale, 1.0);
    assert_eq!(k.with_text_scale(10.0).text_scale, TEXT_SCALES.1);
}

#[test]
fn steps_walk_the_standard_sizes_and_stop_at_the_ends() {
    let s = TextScale::default();
    assert_eq!(s.step(TextStep::Bigger).get(), 1.15);
    assert_eq!(s.step(TextStep::Smaller).get(), 0.85);
    assert_eq!(TextScale::new(1.2).step(TextStep::Bigger).get(), 1.3);
    assert_eq!(TextScale::new(1.2).step(TextStep::Smaller).get(), 1.15);
    assert_eq!(TextScale::new(2.0).step(TextStep::Normal).get(), 1.0);
    let top = TextScale::new(2.5);
    assert_eq!(top.step(TextStep::Bigger).0.id(), top.0.id(), "no step past the largest");
    assert_eq!(TextScale::new(0.75).step(TextStep::Smaller).get(), 0.75);
}

#[test]
fn command_equals_minus_and_zero_are_the_text_size_keys() {
    let cmd = Mods { cmd: true, ..Mods::default() };
    let ev = |c| KeyEvent { key: Key::Command(c), mods: cmd };
    assert_eq!(TextStep::from_key(&ev('=')), Some(TextStep::Bigger));
    assert_eq!(TextStep::from_key(&ev('+')), Some(TextStep::Bigger));
    assert_eq!(TextStep::from_key(&ev('-')), Some(TextStep::Smaller));
    assert_eq!(TextStep::from_key(&ev('0')), Some(TextStep::Normal));
    assert_eq!(TextStep::from_key(&ev('s')), None);
    assert_eq!(TextStep::from_key(&KeyEvent { key: Key::Text("=".into()), mods: Mods::default() }), None);
    let _ = TextScale::menu_items::<App>(|s| &mut s.scale);
}

const CONTROLS: [&str; 7] = ["badge", "keybind", "segmented", "checkbox", "radio", "field", "sidebar_row"];

struct Controls(TextScale);

fn controls(s: &Controls, cx: &mut Cx<Controls>) -> El {
    let theme = KitTheme::neutral();
    cx.with_text_scale(&s.0, |cx| {
        cx.with_kit(&theme, |cx| {
            let k = cx.kit();
            let one = |e: El, key: &str| row().child(e.key(key));
            col()
                .items_start()
                .child(one(Badge::new("New").build(&k), "badge"))
                .child(one(Keybind::new(&["cmd", "K"]).build(&k), "keybind"))
                .child(one(SegmentedControl::new(&["Day", "Week"], Some(0)).build(&k), "segmented"))
                .child(one(Checkbox::new(true).label("Wrap lines").build(&k), "checkbox"))
                .child(one(RadioItem::new("Compact", true).build(&k), "radio"))
                .child(one(Field::new().label("Name").info().prefix("Shown to others").build(&k), "field"))
                .child(el().w(240.0).child(SidebarRow::new("Inbox").build(&k).key("sidebar_row")))
        })
    })
}

fn rendered(scale: f32) -> Vec<rune::ElementInfo> {
    let mut rt = Runtime::new(Controls(TextScale::new(scale)), controls, (600.0, 900.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.settle_all();
    rt.frame(0.0);
    rt.elements()
}

fn control<'a>(els: &'a [rune::ElementInfo], key: &str) -> &'a rune::ElementInfo {
    els.iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key} is rendered"))
}

fn line_boxes<'a>(els: &'a [rune::ElementInfo], of: &'a rune::ElementInfo) -> impl Iterator<Item = &'a rune::ElementInfo> {
    els.iter().filter(move |e| e.text.is_some() && e.path.starts_with(&of.path) && e.path[of.path.len()..].starts_with('/'))
}

#[test]
fn at_twice_the_text_size_every_control_holds_its_text_and_grows_only_by_it() {
    let (plain, big) = (rendered(1.0), rendered(2.0));
    for key in CONTROLS {
        let (a, b) = (control(&plain, key), control(&big, key));
        let [x, y, w, h] = b.rect;
        let mut texts = 0;
        for t in line_boxes(&big, b) {
            texts += 1;
            let [tx, ty, tw, th] = t.rect;
            let line = t.text_size.expect("laid-out text")[1];
            assert!(line <= th + 0.5, "{key}: {:?}'s line box ({line}) fits its element ({th})", t.text);
            assert!(tx >= x - 0.5 && ty >= y - 0.5 && tx + tw <= x + w + 0.5 && ty + th <= y + h + 0.5, "{key}: {:?} at {:?} lies inside {:?}", t.text, t.rect, b.rect);
            let size = t.text_style.as_ref().expect("text style").size;
            let before = line_boxes(&plain, a).find(|p| p.text == t.text).expect("the same text at 1x");
            assert_eq!(size, before.text_style.as_ref().unwrap().size * 2.0, "{key}: {:?} doubles", t.text);
        }
        assert!(texts > 0, "{key} shows text");
        assert!(h > a.rect[3] * 1.5, "{key}'s height ({} to {h}) follows its text", a.rect[3]);
        assert_eq!(b.radius, a.radius, "{key}'s radius stays");
        assert_eq!(b.inset, a.inset, "{key}'s padding stays");
    }
}
