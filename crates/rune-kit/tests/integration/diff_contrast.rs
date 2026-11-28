use std::collections::BTreeSet;

use rune::{Color, Cx, El, Env, Runtime, col};
use rune_kit::Kit;
use rune_kit::diff::{DiffMode, DiffState, DiffView, FileDiff};
use rune_kit::syntax::{Syntax, SyntaxTheme};

use crate::contrast::{Glyph, built_in_kits, glyphs_on_fills, text_floor};

const OLD: &str = "fn serve() {\n    let port = 8080;\n    let host = \"alpha\";\n}\n";
const NEW: &str = "fn serve() {\n    let port = 9090;\n    let host = \"omega\";\n}\n";

struct App {
    diff: DiffState,
    kit: Kit,
    syntax: SyntaxTheme,
}

fn view(s: &App, _: &mut Cx<App>) -> El {
    let diff = DiffView::new(|s: &App| &s.diff, |s: &mut App| &mut s.diff).syntax_theme(s.syntax);
    col().w_full().h_full().child(diff.build(s, &s.kit))
}

fn code_on_tints(kit: &Kit, syntax: SyntaxTheme) -> Vec<Glyph> {
    let mut diff = DiffState::new(vec![FileDiff::from_texts("src/serve.rs", OLD, NEW)]);
    diff.set_mode(DiffMode::Unified);
    let app = App { diff, kit: kit.clone(), syntax };
    let mut rt = Runtime::new(app, view, (720.0, 360.0), Env::new(kit.dark, 1.0));
    rt.set_srgb_blend(false);
    for _ in 0..4 {
        rt.frame(1.0 / 60.0);
    }
    let code: Vec<[f32; 4]> =
        rt.elements().into_iter().filter(|e| e.key.as_deref() == Some("code") && !e.exiting).map(|e| e.rect).collect();
    glyphs_on_fills(&rt.instances, &code, &[kit.c.bg_base])
}

#[test]
fn code_on_change_tints_and_word_pills_reaches_text_contrast_in_every_built_in_theme() {
    let pale_amber = Color { r: 0.92, g: 0.58, b: 0.18, a: 1.0 };
    for (name, kit) in built_in_kits() {
        let floor = text_floor(&kit);
        for (theme_name, syntax) in [
            ("kit syntax", SyntaxTheme::from_kit(&kit)),
            ("pale amber numbers", SyntaxTheme::from_kit(&kit).with(Syntax::Number, pale_amber)),
        ] {
            let glyphs = code_on_tints(&kit, syntax);
            for g in &glyphs {
                assert!(g.ratio >= floor, "{name}, {theme_name}: code glyph {:?} at {:?} is {:.2}:1 on {:?}, under {floor}:1", g.ink, g.at, g.ratio, g.fill);
            }
            let fills: BTreeSet<[u32; 4]> = glyphs.iter().map(|g| g.fill.map(f32::to_bits)).collect();
            assert!(fills.len() >= 4, "{name}, {theme_name}: glyphs checked on added and removed lines and word pills, saw {} fills", fills.len());
        }
    }
}

#[test]
fn increased_contrast_draws_change_tints_further_from_the_background() {
    let standing = |kit: &Kit| {
        let bg = kit.c.bg_base.to_oklab();
        let fills: BTreeSet<[u32; 4]> = code_on_tints(kit, SyntaxTheme::from_kit(kit)).iter().map(|g| g.fill.map(f32::to_bits)).collect();
        let distance = |f: &[u32; 4]| {
            let [r, g, b, _] = f.map(f32::from_bits);
            let lab = Color { r, g, b, a: 1.0 }.to_oklab();
            (0..3).map(|i| (lab[i] - bg[i]).powi(2)).sum::<f32>().sqrt()
        };
        fills.iter().map(distance).fold(f32::MAX, f32::min)
    };
    for (name, kit) in built_in_kits().into_iter().filter(|(_, k)| !k.increased_contrast) {
        let (plain, increased) = (standing(&kit), standing(&kit.increased_contrast()));
        assert!(increased > plain * 1.3, "{name}: the faintest change tint is {increased:.4} from the background in OKLab with increased contrast, {plain:.4} without");
    }
}
