#![forbid(unsafe_code)]

use rune::tok::{self, type_};
use rune::{AppSpec, Autocomplete, Cx, El, ScrollSpec, TextEdit, col, text, text_input};

#[derive(Default)]
struct State {
    name: TextEdit,
    email: TextEdit,
    notes: TextEdit,
    city: TextEdit,
}

fn field(label: &'static str, edit: &TextEdit, t: &tok::Theme) -> El {
    text_input(edit, type_::BODY).key(label).label(label).placeholder(label, t.text_secondary).color(t.text)
        .w_full().min_h(48.0).padding(12.0).radius(12.0).fill(t.card)
}

fn view(state: &State, cx: &mut Cx<State>) -> El {
    let t = tok::theme(cx.system_dark());
    let [top, right, bottom, left] = cx.safe_area();
    let filler = (1..=8).fold(col().w_full().gap(12.0), |list, n| {
        list.child(text(format!("Paragraph {n}: the fields below sit under the keyboard until it scrolls them up."))
            .font(type_::BODY).color(t.text_secondary).wrap().w_full())
    });
    let form = col().w_full().gap(16.0).py(16.0)
        .child(text("Fields").role("heading").level(1).font(type_::TITLE).color(t.text))
        .child(field("Name", &state.name, &t).autocomplete(Autocomplete::Name).on_edit(|s: &mut State| &mut s.name))
        .child(field("Email", &state.email, &t).autocomplete(Autocomplete::Email).on_edit(|s: &mut State| &mut s.email))
        .child(filler)
        .child(field("Notes", &state.notes, &t).spellcheck(true).on_edit(|s: &mut State| &mut s.notes))
        .child(field("City", &state.city, &t).on_edit(|s: &mut State| &mut s.city));
    col().w_full().h_full().pt(top).pr(16.0 + right).pb(bottom).pl(16.0 + left).fill(t.bg)
        .child(col().key("form").w_full().grow(1.0).shrink().scroll(ScrollSpec { stick_bottom: false }).child(form))
}

fn main() {
    rune::App::new(AppSpec::new("Fields", (390.0, 760.0), State::default, view)).run();
}
