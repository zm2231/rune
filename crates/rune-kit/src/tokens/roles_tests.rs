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
fn increased_contrast_lifts_meta_text_as_it_does_faint_text() {
    let k = Kit::neutral(true);
    let more = k.increased_contrast();
    assert_eq!(more.roles.meta, k.roles.meta.mix(k.c.text_base, 0.5));
    assert_eq!(more.roles.link, k.roles.link);
}
