use rune::{Cx, El, Env, Key, KeyEvent, Mods, Runtime, col, el, row};

use super::*;
use crate::{Button, KitCx, KitTheme};

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
