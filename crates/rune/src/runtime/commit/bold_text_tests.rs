use crate::runtime::Runtime;
use crate::{Cx, El, Env, Span, TextStyle, col, memo, text};

use super::bolder;

#[derive(Default)]
struct S {
    bold: Option<bool>,
}

fn view(s: &S, cx: &mut Cx<S>) -> El {
    cx.bold_text(s.bold.unwrap_or(cx.system_bold_text()));
    col()
        .child(text("Plain").font(TextStyle::new(16.0, 400.0)).key("plain"))
        .child(memo("kept", 0, || {
            text("Rich title").font(TextStyle::new(16.0, 500.0)).spans([Span::new(0, 4).weight(600.0)])
        }))
}

fn weights(rt: &Runtime<S>, key: &str) -> (f32, Vec<Option<f32>>) {
    let n = rt.tree.nodes.iter().find(|n| n.key.as_deref() == Some(key)).expect("text mounted");
    let t = n.text.as_ref().expect("text");
    (t.style.weight, t.spans.iter().map(|s| s.weight).collect())
}

#[test]
fn bold_text_draws_each_weight_one_step_heavier_up_to_black() {
    assert_eq!([300.0, 400.0, 500.0, 600.0, 700.0, 800.0, 900.0].map(bolder), [500.0, 600.0, 700.0, 700.0, 800.0, 900.0, 900.0]);
}

#[test]
fn a_view_that_follows_the_systems_bold_text_redraws_its_text_and_its_memos_when_it_changes() {
    let mut rt = Runtime::new(S::default(), view, (300.0, 200.0), Env::new(true, 1.0));
    rt.frame(0.0);
    assert_eq!(weights(&rt, "plain"), (400.0, vec![]));
    assert_eq!(weights(&rt, "kept"), (500.0, vec![Some(600.0)]));
    rt.set_system_bold_text(true);
    rt.frame(0.0);
    assert_eq!(weights(&rt, "plain"), (600.0, vec![]));
    assert_eq!(weights(&rt, "kept"), (700.0, vec![Some(700.0)]), "the memo rebuilt");
    rt.state.bold = Some(false);
    rt.invalidate();
    rt.frame(0.0);
    assert_eq!(weights(&rt, "kept"), (500.0, vec![Some(600.0)]), "the app turned it off");
}
