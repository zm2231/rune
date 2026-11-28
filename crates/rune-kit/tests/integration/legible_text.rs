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
    k.c.text_strong = quiet;
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
        ("text_strong", k.c.text_strong, body),
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
        k.c.text_strong = ink;
        k.roles.link = ink;
        let held = k.legible_text();
        assert_eq!(held.c.text_muted, k.c.text_muted);
        assert_eq!(held.c.text_strong, ink);
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

fn on_surfaces(base: &str, deep: &str, layer: &str, more: bool) -> Kit {
    let mut k = rune_theme_opencode::kit(&KitEnv::new(true).increased_contrast(more));
    k.c.bg_base = hex(base).unwrap();
    k.c.bg_deep = hex(deep).unwrap();
    k.c.layer_01 = hex(layer).unwrap();
    k
}

#[test]
fn faint_text_lifted_for_a_dark_base_still_reads_on_it_after_a_lighter_pane() {
    let mut k = on_surfaces("#000000", "#000000", "#5a5a5a", false);
    k.c.text_faint = hex("#1e1e1e").unwrap();
    let held = k.legible_text();
    for fill in [held.c.bg_base, held.c.bg_deep, held.c.layer_01] {
        assert!(ratio(held.c.text_faint, fill) >= 3.0, "{:?} on {:?}: {}", held.c.text_faint, fill, ratio(held.c.text_faint, fill));
    }
}

#[test]
fn every_text_colour_reads_on_all_three_surfaces_when_they_mix_dark_and_light() {
    for more in [false, true] {
        let mut k = on_surfaces("#000000", "#0a0a0a", "#4a4a4a", more);
        let served = served(&KitEnv::new(true).increased_contrast(more));
        k.c.text_strong = served.c.text_strong;
        k.c.text_base = hex("#1e1e1e").unwrap();
        k.c.text_weak = hex("#3c3c3c").unwrap();
        k.c.text_muted = served.c.text_muted;
        k.c.text_faint = hex("#1e1e1e").unwrap().alpha(0.7);
        k.c.text_accent = served.c.text_accent;
        k.roles = served.roles;
        let held = k.legible_text();
        for (name, ink, min) in text(&held) {
            assert!(worst(&held, ink) >= min, "{name} (more {more}): {}", worst(&held, ink));
        }
        assert_eq!(held.text_short_of_floor(), []);
    }
}

#[test]
fn text_with_no_colour_reading_on_every_surface_gets_the_most_it_can_and_is_named() {
    let held = on_surfaces("#1e1e1e", "#f0f0f0", "#f0f0f0", false).legible_text();
    let short = held.text_short_of_floor();
    let names: Vec<_> = short.iter().map(|s| s.name).collect();
    assert!(names.contains(&"text_base") && names.contains(&"link"), "{names:?}");
    let best = (0..=255u8)
        .map(|v| hex(&format!("#{v:02x}{v:02x}{v:02x}")).unwrap())
        .map(|grey| worst(&held, grey))
        .fold(0.0, f32::max);
    for s in &short {
        assert!(s.lowest < s.floor);
        assert!((s.lowest - worst(&held, s.color)).abs() < 1e-4);
        assert!(s.lowest >= best - 0.05, "{}: {} where a grey reaches {best}", s.name, s.lowest);
    }
}

#[test]
fn legible_on_all_finds_the_grey_between_black_and_white() {
    let red = hex("#e53935").unwrap();
    let held = rune_kit::legible_on_all(red, &[Color::BLACK, Color::WHITE], 4.5);
    assert!(ratio(held, Color::BLACK) >= 4.5 && ratio(held, Color::WHITE) >= 4.5, "{held:?}");
    let [_, a, b, _] = red.to_oklab();
    let [_, ha, hb, _] = held.to_oklab();
    assert!((b.atan2(a) - hb.atan2(ha)).abs() < 0.1, "keeps its hue");
    assert_eq!(rune_kit::legible_on_all(red, &[], 4.5), red);
}

#[test]
fn text_reaches_the_floor_when_one_lightness_alone_meets_it_on_every_fill() {
    let fg = Color { r: 0.1, g: 0.3, b: 0.2, a: 1.0 };
    for v in [0.9625, f32::from_bits(0.9625f32.to_bits() + 1)] {
        let light = Color { r: v, g: v, b: v, a: 1.0 };
        for ink in [Color::BLACK, fg] {
            let held = rune_kit::legible_on_all(ink, &[Color::BLACK, light], 4.5);
            for fill in [Color::BLACK, light] {
                let (a, b) = (held.luminance() + 0.05, fill.luminance() + 0.05);
                assert!(a.max(b) / a.min(b) >= 4.5, "{ink:?} -> {held:?} on {fill:?}: {}", a.max(b) / a.min(b));
            }
        }
    }
}

#[test]
fn text_of_any_hue_reaches_a_floor_only_one_lightness_meets() {
    let raw = |a: Color, b: Color| {
        let (a, b) = (a.luminance() + 0.05, b.luminance() + 0.05);
        a.max(b) / a.min(b)
    };
    for i in 0..400 {
        let v = 0.96249 + i as f32 * 5e-8;
        let light = Color { r: v, g: v, b: v, a: 1.0 };
        let reachable = (0..2000)
            .map(|j| 0.17499 + j as f32 * 1e-8)
            .any(|g| [Color::BLACK, light].iter().all(|&f| raw(Color { r: g, g, b: g, a: 1.0 }, f) >= 4.5));
        if !reachable {
            continue;
        }
        for h in 0..24 {
            let t = h as f32 / 24.0;
            let fg = Color { r: t, g: (1.0 - t) * 0.6, b: (t * 7.0).fract(), a: 1.0 };
            let held = rune_kit::legible_on_all(fg, &[Color::BLACK, light], 4.5);
            let lowest = raw(held, Color::BLACK).min(raw(held, light));
            assert!(lowest >= 4.5, "{fg:?} beside {v}: {held:?} reads {lowest}");
        }
    }
}
