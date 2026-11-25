//! A Rune app on iPhone: a header that keeps clear of the notch and the home indicator, two
//! buttons that tap out haptics, and a long list that scrolls and flings under a finger, where
//! holding a row marks it. `scripts/ios-app.sh ios-hello` builds it for the simulator; off iOS
//! `cargo run -p ios-hello` opens the same view in a window.

#![forbid(unsafe_code)]

use rune::tok;
use rune::{AppSpec, Color, Cx, El, Haptic, HapticOutcome, ImpactStrength, ScrollSpec, TextStyle, Tx, col, row, text};

const ROWS: usize = 80;
const TITLE: TextStyle = TextStyle::new(28.0, 700.0);
const BODY: TextStyle = TextStyle::new(17.0, 400.0);
const CAPTION: TextStyle = TextStyle::new(13.0, 500.0);

#[derive(Default)]
struct State {
    taps: u32,
    picked: Option<usize>,
    held: Option<usize>,
}

fn button(key: &str, label: &str, fill: Color, f: impl Fn(&mut State, &mut Tx) + 'static) -> El {
    row().key(key).role("button").label(label).h(48.0).grow(1.0).center().radius(12.0).fill(fill)
        .child(text(label).font(BODY).color(Color::WHITE))
        .on_press_tx(f)
}

fn header(s: &State, cx: &Cx<State>, t: &tok::Theme) -> El {
    let dark = cx.system_dark();
    let status = format!(
        "{} · {} · touch controls {}",
        if dark { "Dark" } else { "Light" },
        if cx.system_reduced_motion() { "Reduce Motion" } else { "full motion" },
        if cx.touch_controls() { "on" } else { "off" },
    );
    col().key("header").w_full().px(20.0).pb(12.0).gap(12.0)
        .child(text("Rune on iPhone").font(TITLE).color(t.text))
        .child(text(status).font(CAPTION).color(t.text_secondary))
        .child(row().w_full().gap(12.0)
            .child(button("tap", &format!("Tapped {}", s.taps), tok::accent(0, dark), |s: &mut State, tx: &mut Tx| {
                s.taps += 1;
                tx.haptic(Haptic::Impact(ImpactStrength::Light));
            }))
            .child(button("done", "Success", tok::accent(2, dark), |_: &mut State, tx: &mut Tx| {
                tx.haptic(Haptic::Notify(HapticOutcome::Success));
            })))
}

fn list_row(i: usize, s: &State, t: &tok::Theme) -> El {
    let fill = if s.held == Some(i) { t.press } else if s.picked == Some(i) { t.hover } else { t.card };
    let note = if s.held == Some(i) { "held" } else if s.picked == Some(i) { "picked" } else { "" };
    row().key(format!("row-{i}")).w_full().h(52.0).px(20.0).center_y().justify_between().fill(fill)
        .child(text(format!("Row {i}")).font(BODY).color(t.text))
        .child(text(note).font(CAPTION).color(t.text_secondary))
        .on_press_tx(move |s: &mut State, tx: &mut Tx| {
            s.picked = Some(i);
            tx.haptic(Haptic::Selection);
        })
        .on_secondary_press(move |s: &mut State, _, tx: &mut Tx| {
            s.held = Some(i);
            tx.haptic(Haptic::Impact(ImpactStrength::Medium));
        })
}

fn view(s: &State, cx: &mut Cx<State>) -> El {
    let t = tok::theme(cx.system_dark());
    let [top, right, bottom, left] = cx.safe_area();
    let mut list = col().w_full().gap(1.0).fill(t.separator).pb(bottom);
    for i in 0..ROWS {
        list = list.child(list_row(i, s, &t));
    }
    col().w_full().h_full().fill(t.bg).pt(top).pl(left).pr(right)
        .child(header(s, cx, &t))
        .child(col().key("list").w_full().grow(1.0).scroll(ScrollSpec { stick_bottom: false }).child(list))
}

fn spec() -> AppSpec<State> {
    AppSpec::new("Rune on iPhone", (390.0, 844.0), State::default, view)
}

fn main() {
    #[cfg(target_os = "ios")]
    rune_ios::run(spec());
    #[cfg(not(target_os = "ios"))]
    rune::run(spec());
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime, TouchPhase};

    #[test]
    fn the_header_keeps_clear_of_the_safe_area() {
        let mut rt = Runtime::new(State::default(), view, (390.0, 844.0), Env { system_dark: true, scale: 3.0 });
        rt.set_safe_area([59.0, 0.0, 34.0, 0.0]);
        rt.frame(0.0);
        let header = rt.presented("header").unwrap_or_else(|| panic!("no header")).0;
        assert_eq!(header[1], 59.0);
    }

    #[test]
    fn a_finger_dragged_up_scrolls_the_list() {
        let mut rt = Runtime::new(State::default(), view, (402.0, 874.0), Env { system_dark: true, scale: 3.0 });
        rt.frame(0.0);
        let top = |rt: &Runtime<State>| rt.presented("row-0").unwrap_or_else(|| panic!("no row 0")).0[1];
        let before = top(&rt);
        rt.touch(1, TouchPhase::Started, 200.0, 750.0, 0.0);
        for i in 1..=20 {
            rt.touch(1, TouchPhase::Moved, 200.0, 750.0 - i as f32 * 20.0, i as f64 * 0.016);
            rt.frame(0.016);
        }
        rt.touch(1, TouchPhase::Ended, 200.0, 350.0, 0.33);
        rt.frame(0.016);
        assert!(top(&rt) < before - 300.0, "row 0 moved from {before} to {}", top(&rt));
        assert_eq!(rt.state.picked, None, "a drag picks no row");
    }
}
