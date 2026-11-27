//! An app with a second window and an overlay, on every platform. The main window opens a
//! "Details" window, which keeps its own count, and shows a HUD on the bottom edge.
//!
//! Hiding the HUD fades it out, and once it is gone `OverlayHandle::after_hidden` counts the
//! hide in the main window.
//!
//! On the desktop the details window is a window of its own and the HUD floats above other
//! apps. On an iPad built with `scripts/ios-app --multiple-scenes` the details window is a scene
//! of its own; on an iPhone it is a sheet over the main window, which a swipe down closes. On
//! iOS the HUD sits above the home indicator, over the list, and a touch on it outside its
//! button reaches the row under it.
//!
//! `cargo run -p ios-windows`

#![forbid(unsafe_code)]

use std::time::Duration;

use rune::tok::{self, type_};
use rune::{App, AppHandle, AppSpec, Cx, El, OverlayHandle, OverlayInput, OverlayOptions, OverlaySpec, ScreenEdge, ScrollSpec, Tx, WindowOptions, WindowSpec, col, el, row, text};

struct Main {
    count: u32,
    tapped: Option<usize>,
    hud_shown: bool,
    hud_hidden: u32,
    details: Option<AppHandle<Details>>,
    hud: Option<OverlayHandle<Hud>>,
    me: Option<AppHandle<Main>>,
}

#[derive(Default)]
struct Details {
    count: u32,
    closed: u32,
}

#[derive(Default)]
struct Hud {
    taps: u32,
}

fn button<S: 'static>(key: &'static str, label: String, t: &tok::Theme, f: impl Fn(&mut S, &mut Tx) + 'static) -> El {
    el().key(key).role("button").label(label.clone()).focusable(true).px(16.0).py(10.0).radius(10.0).fill(t.card)
        .on_press_tx(f).child(text(label).font(type_::BODY).color(t.text))
}

fn main_view(s: &Main, cx: &mut Cx<Main>) -> El {
    let t = tok::theme(cx.system_dark());
    let [top, right, bottom, left] = cx.safe_area();
    let hud = if s.hud_shown { "Hide HUD" } else { "Show HUD" };
    let controls = row().gap(12.0).wrap()
        .child(button("open-details", "Open details".into(), &t, |s: &mut Main, _| if let Some(d) = &s.details { d.open() }))
        .child(button("toggle-hud", hud.into(), &t, |s: &mut Main, _| {
            s.hud_shown = !s.hud_shown;
            let Some(hud) = &s.hud else { return };
            if s.hud_shown { return hud.show(); }
            hud.fade_out(Duration::from_millis(400));
            let me = s.me.clone();
            hud.after_hidden(move |_, _| if let Some(me) = me { me.update(|s: &mut Main, _| s.hud_hidden += 1) });
        }))
        .child(button("count", format!("Count {}", s.count), &t, |s: &mut Main, _| s.count += 1));
    let tapped = match s.tapped {
        Some(row) => format!("Tapped row {row}"),
        None => "Tap a row, also under the HUD".into(),
    };
    let rows = (0..30).fold(col().w_full().gap(8.0), |list, at| {
        let selected = s.tapped == Some(at);
        list.child(el().key(format!("row-{at}")).w_full().h(48.0).radius(10.0).px(16.0).center_y()
            .fill(if selected { tok::accent(0, cx.system_dark()) } else { t.card })
            .on_press(move |s: &mut Main| s.tapped = Some(at))
            .child(text(format!("Row {at}")).font(type_::BODY).color(t.text)))
    });
    col().w_full().h_full().gap(16.0).pt(top + 16.0).pr(16.0 + right).pb(bottom).pl(16.0 + left).fill(t.bg)
        .child(text("Windows").role("heading").level(1).font(type_::TITLE).color(t.text))
        .child(controls)
        .child(text(tapped).key("tapped").font(type_::BODY).color(t.text_secondary))
        .child(text(format!("HUD hidden {} times", s.hud_hidden)).key("hud-hidden").font(type_::BODY).color(t.text_secondary))
        .child(col().key("rows").w_full().grow(1.0).shrink().scroll(ScrollSpec { stick_bottom: false }).child(rows))
}

fn details_view(s: &Details, cx: &mut Cx<Details>) -> El {
    let t = tok::theme(cx.system_dark());
    let [top, right, bottom, left] = cx.safe_area();
    col().w_full().h_full().center().gap(16.0).pt(top).pr(right).pb(bottom).pl(left).fill(t.bg)
        .child(text("Details").role("heading").level(1).font(type_::TITLE).color(t.text))
        .child(text(format!("Count {}, closed {} times", s.count, s.closed)).key("details-count").font(type_::BODY).color(t.text_secondary))
        .child(row().gap(12.0)
            .child(button("details-add", "Add one".into(), &t, |s: &mut Details, _| s.count += 1))
            .child(button("details-close", "Close".into(), &t, |_: &mut Details, tx| tx.close_window())))
}

fn hud_view(s: &Hud, cx: &mut Cx<Hud>) -> El {
    let t = tok::theme(cx.system_dark());
    row().key("hud").w_full().h_full().radius(28.0).px(20.0).gap(12.0).center_y().fill(t.card.alpha(0.92))
        .child(text("HUD").font(type_::BODY).color(t.text).grow(1.0))
        .child(button("hud-tap", format!("Taps {}", s.taps), &t, |s: &mut Hud, _| s.taps += 1).hit())
}

fn app() -> App<Main> {
    let mut app = App::new(AppSpec::new("Windows", (420.0, 720.0), || Main { count: 0, tapped: None, hud_shown: false, hud_hidden: 0, details: None, hud: None, me: None }, main_view));
    let details = App::new(AppSpec::new("Details", (360.0, 280.0), Details::default, details_view)).on_close(|s: &mut Details, _| s.closed += 1);
    let details = WindowSpec::from_app("details", details)
        .options(WindowOptions { visible: false, ..Default::default() });
    let (details, _) = app.window(details);
    let hud = OverlaySpec { edge: ScreenEdge::Bottom, margin: 24.0, interactive: true, ..OverlaySpec::new("HUD", (300.0, 56.0), hud_view) }
        .options(OverlayOptions { input: OverlayInput::Regions, ..Default::default() });
    let hud = app.overlay(hud, Hud::default());
    let me = app.handle();
    app.init(move || Main { count: 0, tapped: None, hud_shown: false, hud_hidden: 0, details: Some(details), hud: Some(hud), me: Some(me) })
}

fn main() {
    app().run();
}
