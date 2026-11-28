use std::collections::{BTreeMap, BTreeSet, HashMap};

use rune::{Cx, El, Env, Runtime, col};
use rune_kit::diff::{DiffMode, DiffState, DiffView, FileDiff};
use rune_kit::syntax::{Highlight, Highlighter, Paint, Syntax, SyntaxTheme};
use rune_kit::{Kit, Markdown};

use crate::contrast::{built_in_kits, glyphs_within, text_floor};

fn words() -> String {
    (0..Syntax::ALL.len()).map(|i| format!("w{i:02}")).collect::<Vec<_>>().join(" ")
}

fn by_word(_: Option<&str>, code: &str) -> Vec<Highlight> {
    let mut out = Vec::new();
    let mut at = 0;
    for word in code.split(|c: char| !c.is_ascii_alphanumeric()) {
        if let Some(i) = word.strip_prefix('w').and_then(|n| n.parse::<usize>().ok()).filter(|&i| i < Syntax::ALL.len()) {
            out.push((at..at + word.len(), Paint::Kind(Syntax::ALL[i])));
        }
        at += word.len() + 1;
    }
    out
}

struct App {
    kit: Kit,
    diff: DiffState,
}

fn view(s: &App, _: &mut Cx<App>) -> El {
    let fence = format!("```code\n{}\n```", words());
    col()
        .w_full()
        .h_full()
        .gap(12.0)
        .child(Markdown::new(fence).highlighter(Highlighter::new(by_word)).build(&s.kit))
        .child(DiffView::new(|s: &App| &s.diff, |s: &mut App| &mut s.diff).build(s, &s.kit))
}

#[test]
fn every_syntax_token_reaches_text_contrast_on_code_blocks_and_unchanged_diff_lines_in_every_built_in_theme() {
    let old = format!("{}\nend 1\n", words());
    let new = format!("{}\nend 2\n", words());
    for (name, kit) in built_in_kits() {
        let floor = text_floor(&kit);
        let theme = rune::theme::with_blend(false, || SyntaxTheme::from_kit(&kit));
        let tokens: BTreeMap<[u32; 4], Syntax> = Syntax::ALL.iter().map(|&s| (theme.color(s).premul(1.0).map(f32::to_bits), s)).collect();
        assert_eq!(tokens.len(), Syntax::ALL.len(), "{name}: every kind keeps its own colour");
        let mut diff = DiffState::new(vec![FileDiff::from_texts("src/tokens.txt", &old, &new)]);
        diff.set_mode(DiffMode::Unified);
        diff.set_highlighter(Highlighter::new(by_word));
        let mut rt = Runtime::new(App { kit: kit.clone(), diff }, view, (900.0, 600.0), Env::new(kit.dark, 1.0));
        rt.set_srgb_blend(false);
        for _ in 0..4 {
            rt.frame(1.0 / 60.0);
        }
        let mut seen: HashMap<Syntax, BTreeSet<[u32; 4]>> = HashMap::new();
        for g in glyphs_within(&rt.instances, &[[0.0, 0.0, 900.0, 600.0]]) {
            let Some(&kind) = tokens.get(&g.ink.map(f32::to_bits)) else { continue };
            assert!(g.ratio >= floor, "{name}: {kind:?} at {:?} is {:.2}:1 on its surface, under {floor}:1", g.at, g.ratio);
            seen.entry(kind).or_default().insert(g.fill.map(f32::to_bits));
        }
        for kind in Syntax::ALL {
            let surfaces = seen.get(&kind).map_or(0, BTreeSet::len);
            assert!(surfaces >= 2, "{name}: {kind:?} drawn on a code block and an unchanged diff line, saw {surfaces} surfaces");
        }
    }
}
