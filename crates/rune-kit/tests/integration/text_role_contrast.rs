use rune::Color;
use rune_kit::{Kit, PaletteSeed, TextRole};

use crate::contrast::{built_in_kits, text_floor};

fn ratio(a: Color, b: Color) -> f32 {
    let (a, b) = (a.luminance(), b.luminance());
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[test]
fn every_text_role_reaches_text_contrast_on_the_surfaces_text_sits_on_in_every_built_in_theme() {
    let mut short = Vec::new();
    let hex = |c: &str| rune::color::hex(c).unwrap();
    let mut kits = built_in_kits();
    for (bg, fg, accent) in [("#ffffff", "#171717", "#e5484d"), ("#0a0a0a", "#ededed", "#e5484d"), ("#e8e4dc", "#000000", "#d81414")] {
        let kit = Kit::from_palette(PaletteSeed { bg: hex(bg), fg: hex(fg), accent: hex(accent) });
        kits.push((format!("palette {bg} increased contrast"), kit.increased_contrast()));
        kits.push((format!("palette {bg}"), kit));
    }
    for (name, kit) in kits {
        let floor = text_floor(&kit);
        let c = &kit.c;
        for role in TextRole::ALL {
            let ink = kit.roles.get(role);
            if ink.a < 1.0 {
                short.push(format!("{name}: {} has alpha {}", role.name(), ink.a));
            }
            for (surface, fill) in [("bg_base", c.bg_base), ("bg_deep", c.bg_deep), ("layer_01", c.layer_01)] {
                let r = ratio(fill.mix(ink.alpha(1.0), ink.a), fill);
                if r < floor {
                    short.push(format!("{name}: {} on {surface} is {r:.2}:1, under {floor}:1", role.name()));
                }
            }
        }
    }
    assert!(short.is_empty(), "{}", short.join("\n"));
}
