//! A web page hosted in a scrolling Rune pane with `El::native_view`: the page tracks its card
//! through scrolls, a spring collapse and window resizes. `cargo run -p native-view` on macOS;
//! elsewhere the card lays out as an empty box. `-- --snap target/native-view.png` renders it
//! headless.

#![forbid(unsafe_code)]

use rune::tok::{self, motion, type_};
use rune::{AppSpec, Cx, El, NativeView, ScrollSpec, TextEdit, Tx, col, el, row, text, text_input};

#[cfg(any(target_os = "macos", target_os = "ios"))]
const URL: &str = "https://en.wikipedia.org/wiki/Special:Random";

struct State {
    slot: NativeView,
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    web: Option<wry::WebView>,
    open: bool,
    tall: bool,
    note: TextEdit,
}

impl Default for State {
    fn default() -> Self {
        State {
            slot: NativeView::new(),
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            web: None,
            open: true,
            tall: false,
            note: TextEdit::default(),
        }
    }
}

fn button(key: &'static str, label: &'static str, t: &tok::Theme, press: fn(&mut State)) -> El {
    text(label).key(key).role("button").label(label).focusable(true).font(type_::LABEL).color(t.text)
        .padding(10.0).radius(8.0).fill(t.card).hover_fill(t.hover).press_fill(t.press)
        .on_press(move |s: &mut State| press(s))
}

fn paragraph(n: usize, t: &tok::Theme) -> El {
    text(format!("Paragraph {n}. Scroll the pane: the page above moves with its card in every frame, and its edges stop at the pane's."))
        .font(type_::BODY).color(t.text_secondary).wrap().w_full()
}

fn view(s: &State, cx: &mut Cx<State>) -> El {
    let t = tok::theme(cx.system_dark());
    let sidebar = col().key("sidebar").w(200.0).h_full().gap(10.0).padding(16.0).fill(t.card)
        .child(text("Native view").font(type_::TITLE).color(t.text))
        .child(button("collapse", if s.open { "Collapse" } else { "Expand" }, &t, |s| s.open = !s.open))
        .child(button("height", if s.tall { "Shorter" } else { "Taller" }, &t, |s| s.tall = !s.tall))
        .child(text_input(&s.note, type_::BODY).key("note").label("Note").single_line()
            .placeholder("Type here, then in the page", t.text_secondary).color(t.text)
            .w_full().padding(10.0).radius(8.0).fill(t.bg).on_edit(|s: &mut State| &mut s.note));
    let page_h = if s.tall { 560.0 } else { 380.0 };
    let card = col().key("card").w_full().h(if s.open { page_h } else { 0.0 }).clip_soft(0.0).radius(14.0)
        .motion(motion::SNAPPY)
        .child(el().key("page").w_full().h(page_h).radius(14.0).native_view(&s.slot));
    let content = (1..=12).fold(
        col().w_full().gap(16.0).padding(24.0)
            .child(text("A page in a pane").font(type_::TITLE).color(t.text))
            .child(paragraph(0, &t))
            .child(card),
        |list, n| list.child(paragraph(n, &t)),
    );
    row().w_full().h_full().fill(t.bg)
        .child(sidebar)
        .child(col().key("pane").grow(1.0).shrink().h_full().scroll(ScrollSpec { stick_bottom: false }).child(content))
}

fn poll(s: &mut State, _: f64, _: &mut Tx) -> bool {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    if s.web.is_none() && s.slot.in_window() {
        match wry::WebViewBuilder::new().with_url(URL).build_as_child(&s.slot) {
            Ok(web) => s.web = Some(web),
            Err(e) => eprintln!("native-view: no web view: {e}"),
        }
    }
    let _ = s;
    false
}

fn main() {
    rune::run(AppSpec::<State> {
        poll: Some(poll),
        scenes: &[("collapsed", |s| s.open = false), ("tall", |s| s.tall = true)],
        ..AppSpec::new("Native view", (900.0, 640.0), State::default, view)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    fn settle(rt: &mut Runtime<State>) {
        for k in 1..=90 { rt.frame(f64::from(k) / 60.0); }
    }

    #[test]
    fn the_page_collapses_with_its_card_and_comes_back() {
        let mut rt = Runtime::new(State::default(), view, (900.0, 640.0), Env { system_dark: false, scale: 2.0 });
        rt.frame(0.0);
        let shown = rt.native_views()[0];
        assert_eq!(shown.view, rt.state.slot.id());
        let (x, y) = rt.find("collapse").unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.05);
        rt.frame(0.1);
        let closing = rt.native_views()[0];
        assert_eq!(closing.rect[3], shown.rect[3], "the page keeps its size");
        assert!(closing.clip[3] < shown.clip[3], "while the card's clip closes over it");
        settle(&mut rt);
        assert!(rt.native_views().is_empty(), "hidden once the card is closed");
        let (x, y) = rt.find("collapse").unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        settle(&mut rt);
        assert_eq!(rt.native_views()[0].clip, shown.clip);
    }
}
