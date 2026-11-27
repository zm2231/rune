//! Two windows over one list: a checklist and a summary. Both show and change the same
//! `Tasks`, each with its own view, focus and scroll. Checking an item in one window updates
//! the other on its next frame; each window knows which one it is through `Cx::window`.
//!
//! On the desktop they are two windows. On an iPad built with `scripts/ios-app
//! --multiple-scenes` each is a scene of its own; on an iPhone the summary is a sheet over the
//! checklist.
//!
//! `cargo run -p shared-windows`

#![forbid(unsafe_code)]

use rune::tok::{self, type_};
use rune::{AppSpec, Cx, El, ScrollSpec, SharedState, Tx, WindowSpec, col, el, row, text};

#[derive(Default)]
struct Tasks {
    items: Vec<(String, bool)>,
    added: u32,
    turns: Vec<String>,
}

impl Tasks {
    fn done(&self) -> usize { self.items.iter().filter(|(_, done)| *done).count() }
}

fn button(key: &'static str, label: &str, t: &tok::Theme, f: impl Fn(&mut Tasks, &mut Tx) + 'static) -> El {
    el().key(key).role("button").label(label).focusable(true).px(16.0).py(10.0).radius(10.0).fill(t.card)
        .on_press_tx(move |s: &mut Tasks, tx| {
            f(s, tx);
            note_change(s, tx);
        })
        .child(text(label).font(type_::BODY).color(t.text))
}

fn frame(title: &str, cx: &mut Cx<Tasks>) -> (El, tok::Theme) {
    let t = tok::theme(cx.system_dark());
    let [top, right, bottom, left] = cx.safe_area();
    let window = cx.window().unwrap_or("window").to_owned();
    let page = col().w_full().h_full().gap(16.0).pt(top + 16.0).pr(16.0 + right).pb(bottom + 16.0).pl(16.0 + left).fill(t.bg)
        .child(text(title).role("heading").level(1).font(type_::TITLE).color(t.text))
        .child(text(format!("This is the \"{window}\" window")).key("which").font(type_::BODY).color(t.text_secondary));
    (page, t)
}

fn checklist(s: &Tasks, cx: &mut Cx<Tasks>) -> El {
    let (page, t) = frame("Checklist", cx);
    let rows = s.items.iter().enumerate().fold(col().w_full().gap(8.0), |list, (at, (name, done))| {
        let mark = if *done { "Done" } else { "To do" };
        list.child(row().key(format!("item-{at}")).w_full().h(48.0).radius(10.0).px(16.0).gap(12.0).center_y()
            .role("checkbox").label(name.clone()).checked(*done).focusable(true)
            .fill(if *done { tok::accent(0, cx.system_dark()) } else { t.card })
            .on_press_tx(move |s: &mut Tasks, tx| {
                s.items[at].1 = !s.items[at].1;
                note_change(s, tx);
            })
            .child(text(name.clone()).font(type_::BODY).color(t.text).grow(1.0))
            .child(text(mark).font(type_::BODY).color(t.text_secondary)))
    });
    page.child(button("add", "Add item", &t, |s, _| {
        s.added += 1;
        s.items.push((format!("Item {}", s.added), false));
    }))
    .child(col().key("items").w_full().grow(1.0).shrink().scroll(ScrollSpec { stick_bottom: false }).child(rows))
}

fn summary(s: &Tasks, cx: &mut Cx<Tasks>) -> El {
    let (page, t) = frame("Summary", cx);
    let last = s.turns.last().map_or("none yet".to_owned(), |key| format!("the \"{key}\" window"));
    page.child(text(format!("{} of {} done", s.done(), s.items.len())).key("count").font(type_::TITLE).color(t.text))
        .child(text(format!("Last change came from {last}")).key("last").font(type_::BODY).color(t.text_secondary))
        .child(button("clear", "Clear done", &t, |s, _| s.items.retain(|(_, done)| !done)))
}

fn note_change(s: &mut Tasks, tx: &Tx) {
    if let Some(key) = tx.window() && s.turns.last().map(String::as_str) != Some(key) {
        s.turns.push(key.to_owned());
    }
}

fn spec(title: &'static str, size: (f32, f32), view: fn(&Tasks, &mut Cx<Tasks>) -> El) -> AppSpec<Tasks> {
    AppSpec::new(title, size, Tasks::default, view)
}

fn main() {
    let start = Tasks { items: vec![("Write the plan".into(), true), ("Review it".into(), false), ("Ship it".into(), false)], added: 3, turns: Vec::new() };
    let tasks = SharedState::new(start);
    rune::run_windows(move |_| [
        WindowSpec::new("checklist", spec("Checklist", (380.0, 520.0), checklist)).shared(&tasks),
        WindowSpec::new("summary", spec("Summary", (320.0, 240.0), summary)).shared(&tasks),
    ]);
}
