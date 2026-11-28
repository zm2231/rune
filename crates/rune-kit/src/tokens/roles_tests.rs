use rune::Color;

use super::*;
use crate::Kit;

const A: Color = Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
const B: Color = Color { r: 0.0, g: 1.0, b: 0.0, a: 1.0 };
const C: Color = Color { r: 0.0, g: 0.0, b: 1.0, a: 1.0 };
const D: Color = Color { r: 1.0, g: 1.0, b: 0.0, a: 1.0 };

#[test]
fn every_role_has_a_name_it_is_found_by_and_its_own_slot() {
    let mut roles = Kit::neutral(true).roles;
    for (i, role) in TextRole::ALL.into_iter().enumerate() {
        assert_eq!(TextRole::named(role.name()), Some(role));
        let mark = Color { r: i as f32 / 10.0, g: 0.5, b: 0.5, a: 1.0 };
        roles.set(role, mark);
        assert_eq!(roles.get(role), mark);
    }
    for (i, role) in TextRole::ALL.into_iter().enumerate() {
        assert_eq!(roles.get(role).r, i as f32 / 10.0, "{role:?} kept its own colour");
    }
    assert_eq!(TextRole::named("timestamp"), None);
}

#[test]
fn a_role_colour_follows_appearance_and_increased_contrast() {
    let plain = RoleColor::new(A, B);
    let lifted = plain.increased(C, D);
    let dark = KitEnv::new(true);
    let light = KitEnv::new(false);
    assert_eq!((plain.pick(&dark), plain.pick(&light)), (A, B));
    assert_eq!(plain.pick(&dark.increased_contrast(true)), A, "no increased pair keeps the plain one");
    assert_eq!((lifted.pick(&dark), lifted.pick(&light)), (A, B));
    assert_eq!((lifted.pick(&dark.increased_contrast(true)), lifted.pick(&light.increased_contrast(true))), (C, D));
}

#[test]
fn increased_contrast_lifts_palette_roles_and_keeps_ones_a_theme_chose() {
    let k = Kit::neutral(true);
    let more = k.increased_contrast();
    let contrast = crate::tokens::palette::contrast;
    assert!(contrast(more.roles.meta, k.c.bg_base) > contrast(k.roles.meta, k.c.bg_base), "meta lifts with faint text");

    let meta = RoleColor::new(A, B).increased(C, D);
    let theme = crate::KitTheme::new(move |env| {
        let mut kit = Kit::neutral(env.dark);
        kit.roles.meta = meta.pick(env);
        if env.increased_contrast { kit.increased_contrast() } else { kit }
    });
    let env = KitEnv::new(true).increased_contrast(true);
    assert_eq!(theme.kit(&env).roles.meta, C, "an increased-contrast colour the app picked is used as given");
    assert_eq!(theme.kit(&KitEnv::new(false)).roles.meta, B);
}

#[test]
fn kit_text_takes_the_code_link_and_error_roles() {
    let mut k = Kit::neutral(true);
    k.roles.code = A;
    k.roles.link = B;
    k.roles.error = C;
    let rich = crate::controls::text_input::kit_rich_style(&k);
    assert_eq!((rich.link, rich.code_fg), (B, Some(A)));
    assert_eq!(crate::controls::text_input::kit_rich_style(&Kit::neutral(true)).code_fg, None, "code at body ink inherits the run's colour");

    struct App(rune::TextEdit, Kit);
    fn view(s: &App, _: &mut rune::Cx<App>) -> rune::El {
        rune::col().w(300.0).child(crate::TextInput::new(&s.0).invalid(true).plain().build(&s.1))
    }
    let mut rt = rune::Runtime::new(App(rune::TextEdit::new("wrong"), k), view, (300.0, 100.0), rune::Env::new(true, 1.0));
    rt.frame(0.0);
    let input = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("input")).expect("the field's text");
    assert_eq!(input.text_color, Some(C), "invalid text takes the error role");
}
