#![cfg_attr(not(target_os = "ios"), forbid(unsafe_code))]
#![cfg_attr(target_os = "ios", deny(unsafe_code))]

#[cfg(target_os = "ios")]
mod host;

use rune::tok::{self, type_};
use rune::{AppSpec, Cx, El, ScrollSpec, TextStyle, col, el, oklch, text};

const BIG: TextStyle = TextStyle::new(34.0, 700.0);
const NAME: TextStyle = TextStyle::new(20.0, 600.0);
const BODY: TextStyle = TextStyle::new(17.0, 400.0);

pub struct State {
    cards: Vec<u32>,
}

impl Default for State {
    fn default() -> Self {
        Self { cards: (1..=12).collect() }
    }
}

pub fn spec() -> AppSpec<State> {
    AppSpec::new("Cards", (390.0, 760.0), State::default, view)
}

fn hue(number: u32) -> f32 {
    (number as f32 * 47.0) % 360.0
}

fn card(number: u32, dark: bool) -> El {
    let fill = if dark { oklch(0.42, 0.12, hue(number)) } else { oklch(0.86, 0.09, hue(number)) };
    let ink = if dark { oklch(0.97, 0.02, hue(number)) } else { oklch(0.28, 0.08, hue(number)) };
    let card = el().key(format!("card-{number}")).role("button").label(format!("Card {number}"))
        .w_full().h(96.0).padding(20.0).radius(22.0).fill(fill).press_scale(0.97)
        .child(text(format!("Card {number}")).font(NAME).color(ink));
    #[cfg(target_os = "ios")]
    let card = card.on_press(move |_: &mut State| host::opened(number));
    card
}

fn view(state: &State, cx: &mut Cx<State>) -> El {
    cx.zoom(cx.system_text_scale());
    let dark = cx.system_dark();
    let t = tok::theme(dark);
    let [top, right, bottom, left] = cx.safe_area();
    let mut list = col().w_full().gap(12.0).pt(16.0 + top).pr(16.0 + right).pb(16.0 + bottom).pl(16.0 + left)
        .child(text("Drawn by Rune").font(BIG).color(t.text))
        .child(text(format!("{} cards. Add them from the toolbar; tap one to open it in SwiftUI.", state.cards.len()))
            .font(BODY).color(t.text_secondary).wrap().w_full());
    if state.cards.is_empty() {
        list = list.child(text("No cards").key("empty").font(type_::TITLE).color(t.text_secondary).w_full().center().h(96.0));
    }
    for &number in &state.cards {
        list = list.child(card(number, dark));
    }
    col().key("cards").w_full().h_full().fill(t.bg).scroll(ScrollSpec { stick_bottom: false }).child(list)
}
