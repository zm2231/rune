#![forbid(unsafe_code)]

use rune::tok::{self, radius, type_};
use rune::{AppSpec, Cx, El, col, el, text};

#[derive(Default)]
struct State {
    presses: usize,
    samples: usize,
    last: [f32; 2],
    total: [f32; 2],
}

fn view(s: &State, cx: &mut Cx<State>) -> El {
    let dark = cx.system_dark();
    let (t, accent) = (tok::theme(dark), tok::accent(0, dark));
    let [top, right, bottom, left] = cx.safe_area();
    let locked = cx.pointer_locked();
    let status = if locked {
        "Locked: move the mouse or trackpad. Escape on a hardware keyboard, or leaving the app, releases it."
    } else if s.presses == 0 {
        "Click here with a mouse or trackpad to lock the pointer. A finger's tap doesn't lock."
    } else {
        "Not locked. If a click left it here, iPadOS refused: the window must be full screen and in front."
    };
    let readout = format!(
        "presses {}  ·  deltas {}  ·  last ({:.1}, {:.1})  ·  total ({:.0}, {:.0})",
        s.presses, s.samples, s.last[0], s.last[1], s.total[0], s.total[1],
    );
    let viewport = col()
        .key("viewport")
        .w_full()
        .grow(1.0)
        .center()
        .gap(12.0)
        .padding(24.0)
        .radius(radius::MD)
        .fill(t.card)
        .outline(if locked { 2.0 } else { 1.0 }, 0.0, if locked { accent } else { t.card_border })
        .on_press(|s: &mut State| s.presses += 1)
        .pointer_lock(|s: &mut State, [dx, dy], _| {
            s.samples += 1;
            s.last = [dx, dy];
            s.total = [s.total[0] + dx, s.total[1] + dy];
        })
        .child(text(if locked { "Locked" } else { "Free" }).key("state").font(type_::TITLE).color(if locked { accent } else { t.text }))
        .child(text(status).key("status").font(type_::LABEL).color(t.text_secondary))
        .child(text(readout).key("readout").font(type_::LABEL).color(t.text));
    col()
        .w_full()
        .h_full()
        .pt(top + 16.0)
        .pr(right + 16.0)
        .pb(bottom + 16.0)
        .pl(left + 16.0)
        .fill(t.bg)
        .child(el().w_full().grow(1.0).child(viewport))
}

fn main() {
    rune::App::new(AppSpec::new("Pointer", (820.0, 1180.0), State::default, view)).run();
}
