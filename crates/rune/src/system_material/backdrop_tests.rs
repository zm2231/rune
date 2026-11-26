use crate::render::{Instance, KIND_BACKDROP_BLUR, KIND_RECT, KIND_SHADOW};
use crate::{Color, Cx, El, Env, Runtime, SystemMaterial, el};

const RECT: [f32; 4] = [60.0, 40.0, 240.0, 120.0];

fn card(fill: Color) -> El {
    el().absolute(30.0, 20.0).size(120.0, 60.0).radius(18.0).shadow(8.0, 24.0, Color::BLACK.alpha(0.4))
        .fill(fill).system_material(SystemMaterial::Glass)
}

fn runtime(view: fn(&Color, &mut Cx<Color>) -> El, fill: Color, dark: bool) -> Runtime<Color> {
    let mut rt = Runtime::new(fill, view, (200.0, 120.0), Env { scale: 2.0, system_dark: dark });
    rt.set_srgb_blend(false);
    rt.core_mut().set_backdrop_materials(true);
    rt.frame(0.0);
    rt
}

fn view(fill: &Color, _: &mut Cx<Color>) -> El { el().size(200.0, 120.0).child(card(*fill)) }

fn kind(rt: &Runtime<Color>, kind: f32) -> Vec<Instance> {
    rt.instances.iter().filter(|q| q.params[1] == kind).copied().collect()
}

#[test]
fn a_material_blurs_what_is_behind_it_under_its_tint() {
    for dark in [false, true] {
        let rt = runtime(view, Color::TRANSPARENT, dark);
        let blurs = kind(&rt, KIND_BACKDROP_BLUR);
        assert_eq!(blurs.len(), 1, "dark {dark}");
        assert_eq!(blurs[0].rect, RECT);
        assert_eq!(blurs[0].params[0], super::sigma(SystemMaterial::Glass) * 2.0);
        let tint = super::tint(SystemMaterial::Glass, Color::TRANSPARENT, dark).premul(1.0);
        assert!(kind(&rt, KIND_RECT).iter().any(|q| q.rect == RECT && q.fill == tint), "dark {dark}");
        let shadow = kind(&rt, KIND_SHADOW)[0];
        assert_eq!([shadow.rect[0] + shadow.uv[0], shadow.rect[1] + shadow.uv[1], shadow.uv[2], shadow.uv[3]], RECT);
    }
}

#[test]
fn a_translucent_fill_tints_the_blur_and_an_opaque_one_draws_alone() {
    let red = Color { r: 1.0, g: 0.0, b: 0.0, a: 0.4 };
    let rt = runtime(view, red, false);
    assert_eq!(kind(&rt, KIND_BACKDROP_BLUR).len(), 1);
    assert!(kind(&rt, KIND_RECT).iter().any(|q| q.rect == RECT && q.fill == red.premul(1.0)));

    let rt = runtime(view, Color { a: 1.0, ..red }, false);
    assert!(kind(&rt, KIND_BACKDROP_BLUR).is_empty());
    assert!(kind(&rt, KIND_RECT).iter().any(|q| q.rect == RECT && q.fill == Color { a: 1.0, ..red }.premul(1.0)));
}

#[test]
fn light_and_dark_tints_differ_and_stay_translucent() {
    for material in [SystemMaterial::Glass, SystemMaterial::Popover, SystemMaterial::Sidebar] {
        let (light, dark) = (super::tint(material, Color::TRANSPARENT, false), super::tint(material, Color::TRANSPARENT, true));
        assert!(light.r > dark.r, "{material:?}");
        assert!(light.a < 1.0 && dark.a < 1.0, "{material:?}");
    }
}
