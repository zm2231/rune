use std::collections::BTreeSet;

use rune::{Cx, El, Env, Runtime, col};
use rune_kit::markdown::MarkdownStyle;
use rune_kit::{Kit, Markdown, Note};

use crate::contrast::{built_in_kits, glyphs_on_fills};

const NOTE: &str = "```rust title=\"serve.rs\"\nfn serve() {\n-    let port = 8080;\n+    let port = 9090;\n}\n```";
const PATCH: &str = "```diff\n-let port = 8080;\n+let port = 9090;\n```";

fn view(k: &Kit, _: &mut Cx<Kit>) -> El {
    col()
        .w_full()
        .gap(12.0)
        .child(Note::new(NOTE).build(k))
        .child(Markdown::new(PATCH).style(MarkdownStyle::from_kit(k).diff_text_tint(1.0)).build(k))
}

#[test]
fn note_code_and_markdown_diff_lines_reach_text_contrast_on_their_tints_in_every_built_in_theme() {
    for (name, kit) in built_in_kits() {
        let plain = [kit.c.bg_base, kit.c.bg_deep, kit.c.layer_01, kit.c.layer_02];
        let mut rt = Runtime::new(kit.clone(), view, (640.0, 480.0), Env::new(kit.dark, 1.0));
        rt.set_srgb_blend(false);
        for _ in 0..4 {
            rt.frame(1.0 / 60.0);
        }
        let glyphs = glyphs_on_fills(&rt.instances, &[[0.0, 0.0, 640.0, 480.0]], &plain);
        for g in &glyphs {
            assert!(g.ratio >= 4.5, "{name}: glyph at {:?} is {:.2}:1 on its tint", g.at, g.ratio);
        }
        let fills: BTreeSet<[u32; 4]> = glyphs.iter().map(|g| g.fill.map(f32::to_bits)).collect();
        assert!(fills.len() >= 4, "{name}: glyphs checked on both blocks' added and removed lines, saw {} fills", fills.len());
    }
}
