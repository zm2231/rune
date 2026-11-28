use std::collections::BTreeSet;

use rune::{Color, Cx, El, Env, Runtime, col};
use rune_kit::Kit;
use rune_kit::diff::{DiffMode, DiffState, DiffView, FileDiff};
use rune_kit::syntax::{Syntax, SyntaxTheme};

use crate::contrast::{built_in_kits, glyphs_on_fills};

struct App {
    diff: DiffState,
    kit: Kit,
    syntax: SyntaxTheme,
}

fn view(s: &App, _: &mut Cx<App>) -> El {
    let diff = DiffView::new(|s: &App| &s.diff, |s: &mut App| &mut s.diff).syntax_theme(s.syntax);
    col().w_full().h_full().child(diff.build(s, &s.kit))
}

#[test]
fn code_on_change_tints_and_word_pills_reaches_text_contrast_in_every_built_in_theme() {
    let old = "fn serve() {\n    let port = 8080;\n    let host = \"alpha\";\n}\n";
    let new = "fn serve() {\n    let port = 9090;\n    let host = \"omega\";\n}\n";
    let pale_amber = Color { r: 0.92, g: 0.58, b: 0.18, a: 1.0 };
    for (name, kit) in built_in_kits() {
        for (theme_name, syntax) in [
            ("kit syntax", SyntaxTheme::from_kit(&kit)),
            ("pale amber numbers", SyntaxTheme::from_kit(&kit).with(Syntax::Number, pale_amber)),
        ] {
            let mut diff = DiffState::new(vec![FileDiff::from_texts("src/serve.rs", old, new)]);
            diff.set_mode(DiffMode::Unified);
            let app = App { diff, kit: kit.clone(), syntax };
            let mut rt = Runtime::new(app, view, (720.0, 360.0), Env::new(kit.dark, 1.0));
            rt.set_srgb_blend(false);
            for _ in 0..4 {
                rt.frame(1.0 / 60.0);
            }
            let code: Vec<[f32; 4]> =
                rt.elements().into_iter().filter(|e| e.key.as_deref() == Some("code") && !e.exiting).map(|e| e.rect).collect();
            let glyphs = glyphs_on_fills(&rt.instances, &code, &[kit.c.bg_base]);
            for g in &glyphs {
                assert!(g.ratio >= 4.5, "{name}, {theme_name}: code glyph at {:?} is {:.2}:1 on its fill", g.at, g.ratio);
            }
            let fills: BTreeSet<[u32; 4]> = glyphs.iter().map(|g| g.fill.map(f32::to_bits)).collect();
            assert!(fills.len() >= 4, "{name}, {theme_name}: glyphs checked on added and removed lines and word pills, saw {} fills", fills.len());
        }
    }
}
