use rune::Color;
use rune::color::hex;
use rune_kit::{Kit, KitEnv, TextRole};

fn ratio(ink: Color, fill: Color) -> f32 {
    let ink = fill.mix(ink.alpha(1.0), ink.a);
    let (a, b) = (ink.luminance() + 0.05, fill.luminance() + 0.05);
    a.max(b) / a.min(b)
}

fn worst(k: &Kit, ink: Color) -> f32 {
    [k.c.bg_base, k.c.bg_deep, k.c.layer_01].into_iter().map(|fill| ratio(ink, fill)).fold(f32::INFINITY, f32::min)
}

fn served(env: &KitEnv) -> Kit {
    let mut k = rune_theme_opencode::kit(env);
    let [quiet, accent, role] =
        if env.dark { ["#5c5c66", "#3a5fcd", "#7a3b3b"] } else { ["#a8a8b0", "#8fb3ff", "#e0b060"] }.map(|c| hex(c).unwrap());
    k.c.text_base = quiet;
    k.c.text_weak = quiet;
    k.c.text_muted = quiet;
    k.c.text_faint = quiet.alpha(0.6);
    k.c.text_accent = accent;
    for r in TextRole::ALL {
        k.roles.set(r, role);
    }
    k
}

fn text(k: &Kit) -> Vec<(&'static str, Color, f32)> {
    let (body, faint) = if k.increased_contrast { (7.0, 4.5) } else { (4.5, 3.0) };
    let mut out = vec![
        ("text_base", k.c.text_base, body),
        ("text_weak", k.c.text_weak, body),
        ("text_muted", k.c.text_muted, body),
        ("text_accent", k.c.text_accent, body),
        ("text_faint", k.c.text_faint, faint),
    ];
    out.extend(TextRole::ALL.map(|r| (r.name(), k.roles.get(r), body)));
    out
}

#[test]
fn an_apps_served_text_colours_fall_short_until_held_to_the_kits_floor() {
    for dark in [false, true] {
        for more in [false, true] {
            let app = served(&KitEnv::new(dark).increased_contrast(more));
            for (name, ink, min) in text(&app) {
                assert!(worst(&app, ink) < min, "{name} (dark {dark}, more {more}) already reads at {}", worst(&app, ink));
            }
            let held = app.legible_text();
            for (name, ink, min) in text(&held) {
                assert!(worst(&held, ink) >= min, "{name} (dark {dark}, more {more}): {}", worst(&held, ink));
            }
            assert_eq!(held.c.text_strong, app.c.text_strong, "text that already reads stays");
            assert_eq!(held.c.icon_base, app.c.icon_base);
            let again = held.legible_text();
            assert_eq!(format!("{:?}", again.c), format!("{:?}", held.c), "palette (dark {dark}, more {more})");
            assert_eq!(again.roles, held.roles);
        }
    }
}

#[test]
fn text_an_app_set_that_already_reads_is_left_as_it_is() {
    for dark in [false, true] {
        let mut k = rune_theme_opencode::kit(&KitEnv::new(dark));
        let ink = if dark { Color::WHITE } else { Color::BLACK };
        k.c.text_muted = ink.alpha(0.95);
        k.roles.link = ink;
        let held = k.legible_text();
        assert_eq!(held.c.text_muted, k.c.text_muted);
        assert_eq!(held.roles.link, ink);
    }
}

#[test]
fn legible_keeps_a_colour_that_reads_and_darkens_one_that_does_not() {
    let blue = hex("#8fb3ff").unwrap();
    let held = rune_kit::legible(blue, Color::WHITE, 4.5);
    assert!(ratio(held, Color::WHITE) >= 4.5);
    let [_, a, b, _] = blue.to_oklab();
    let [_, ha, hb, _] = held.to_oklab();
    assert!((b.atan2(a) - hb.atan2(ha)).abs() < 0.1, "keeps its hue");
    let navy = hex("#1d3a8a").unwrap();
    assert_eq!(rune_kit::legible(navy, Color::WHITE, 4.5), navy);
}
