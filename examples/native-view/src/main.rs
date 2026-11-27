//! A web page hosted in a scrolling Rune pane with `El::native_view`: the page tracks its card
//! through scrolls, a spring collapse and window resizes. Tab moves from the sidebar into the
//! page and back out; on iOS a page script moves Tab through the page. `cargo run -p
//! native-view` on macOS; `-- --tab-lab` loads a local page of form fields that lists each key
//! and focus change it sees, to check Tab handoff against; elsewhere the card lays out as an
//! empty box. `-- --snap target/native-view.png` renders it headless.

#![deny(unsafe_code)]

#[cfg(target_os = "macos")]
mod ax_check;
mod call;

use std::cell::{Cell, OnceCell};
use std::time::Duration;

use rune::tok::{self, motion, type_};
use rune::{App, AppSpec, Cx, El, NativeView, ScrollSpec, SizeClass, TextEdit, Tx, WindowHandle, col, el, row, text, text_input};

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub(crate) const URL: &str = "https://en.wikipedia.org/wiki/Special:Random";

#[cfg(target_os = "ios")]
const TAB_STEP: &str = r#"window.__runeTab = back => {
  const stops = [...document.querySelectorAll('a[href], area[href], button, input, select, textarea, summary, [contenteditable]:not([contenteditable="false"]), [tabindex]')]
    .filter(el => el.tabIndex >= 0 && !el.disabled && el.type !== "hidden" && el.getClientRects().length > 0);
  const rank = el => el.tabIndex > 0 ? el.tabIndex : Infinity;
  const order = stops.map((el, i) => [el, i]).sort(([a, i], [b, j]) => (rank(a) - rank(b)) || (i - j)).map(([el]) => el);
  const at = order.indexOf(document.activeElement), step = back ? -1 : 1;
  for (let k = at < 0 ? (back ? order.length - 1 : 0) : at + step; k >= 0 && k < order.length; k += step) {
    order[k].focus();
    if (document.activeElement === order[k]) return;
  }
  if (document.activeElement && document.activeElement.blur) document.activeElement.blur();
  window.ipc.postMessage(back ? "tab-out-back" : "tab-out");
};"#;

#[cfg(any(target_os = "macos", target_os = "ios"))]
const TAB_LAB: &str = r##"<!doctype html>
<meta name="viewport" content="width=device-width">
<style>
body { font: 16px -apple-system, sans-serif; margin: 12px; }
input, button, a { font: inherit; display: block; margin: 6px 0; }
:focus { outline: 3px solid #f60; }
#log { font: 12px ui-monospace, monospace; white-space: pre; }
</style>
<input id="first" placeholder="first field">
<a id="link" href="#link">a link</a>
<button id="button">a button</button>
<input id="last" placeholder="last field">
<div id="log"></div>
<script>
const out = document.getElementById("log");
const say = line => { out.textContent = (line + "\n" + out.textContent).slice(0, 3000); };
const name = el => el && el.id ? "#" + el.id : el ? el.tagName.toLowerCase() : "none";
addEventListener("keydown", e => say(`keydown ${e.shiftKey ? "Shift-" : ""}${e.key} at ${name(document.activeElement)}`), true);
addEventListener("focusin", e => say(`focusin ${name(e.target)}`), true);
addEventListener("focusout", e => say(`focusout ${name(e.target)}`), true);
addEventListener("focus", () => say("page focus"));
addEventListener("blur", () => say("page blur"));
</script>
"##;

struct State {
    slot: NativeView,
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    web: Option<std::rc::Rc<wry::WebView>>,
    open: bool,
    tall: bool,
    note: TextEdit,
    page_at: Option<f64>,
    seconds: u64,
    views: Cell<u64>,
    background: bool,
}

thread_local! {
    static WINDOW: OnceCell<WindowHandle> = const { OnceCell::new() };
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
            page_at: None,
            seconds: 0,
            views: Cell::new(0),
            background: background(),
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
    s.views.set(s.views.get() + 1);
    let t = tok::theme(cx.system_dark());
    let seconds = match (s.page_at, s.background) {
        (Some(_), true) => format!("Up for {} s", s.seconds),
        (Some(_), false) => format!("Page up for {} s", s.seconds),
        (None, _) => "No page yet".into(),
    };
    let [top, right, bottom, left] = cx.safe_area();
    let compact = cx.horizontal_size_class() == Some(SizeClass::Compact);
    let collapse = button("collapse", if s.open { "Collapse" } else { "Expand" }, &t, |s| s.open = !s.open);
    let height = button("height", if s.tall { "Shorter" } else { "Taller" }, &t, |s| s.tall = !s.tall);
    let note = text_input(&s.note, type_::BODY).key("note").label("Note").single_line()
        .placeholder("Type here, then in the page", t.text_secondary).color(t.text)
        .w_full().padding(10.0).radius(8.0).fill(t.bg).on_edit(|s: &mut State| &mut s.note);
    let title = text("Native view").font(type_::TITLE).color(t.text);
    let seconds = text(seconds).key("seconds").font(type_::BODY).color(t.text_secondary);
    let controls = if compact {
        col().key("controls").w_full().gap(10.0).padding(16.0).fill(t.card)
            .child(title)
            .child(seconds)
            .child(row().gap(10.0).child(collapse).child(height))
            .child(note)
    } else {
        col().key("sidebar").w(200.0).h_full().gap(10.0).padding(16.0).fill(t.card)
            .child(title).child(seconds).child(collapse).child(height).child(note)
    };
    let page_h = if s.tall { 560.0 } else { 380.0 };
    let card = col().key("card").w_full().h(if s.open { page_h } else { 0.0 }).clip_soft(0.0).radius(14.0)
        .motion(motion::SNAPPY)
        .child(el().key("page").w_full().h(page_h).radius(14.0).native_view(&s.slot));
    let inset = if compact { 16.0 } else { 24.0 };
    let content = (1..=12).fold(
        col().w_full().gap(16.0).padding(inset).pb(inset + bottom)
            .child(text("A page in a pane").font(type_::TITLE).color(t.text))
            .child(paragraph(0, &t))
            .child(card),
        |list, n| list.child(paragraph(n, &t)),
    );
    let pane = col().key("pane").grow(1.0).shrink().scroll(ScrollSpec { stick_bottom: false }).child(content);
    let root = if compact { col().child(controls).child(pane.w_full()) } else { row().child(controls).child(pane.h_full()) };
    root.w_full().h_full().fill(t.bg).pt(top).pr(right).pl(left)
}

fn poll(s: &mut State, now: f64, tx: &mut Tx) -> bool {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    if s.web.is_none() && s.slot.in_window() {
        let builder = wry::WebViewBuilder::new();
        #[cfg(target_os = "ios")]
        let builder = {
            let slot = s.slot.clone();
            builder.with_initialization_script(TAB_STEP).with_ipc_handler(move |message: wry::http::Request<String>| {
                match message.body().as_str() {
                    "tab-out" => slot.tabbed_out(false),
                    "tab-out-back" => slot.tabbed_out(true),
                    _ => {}
                }
            })
        };
        let builder = if std::env::args().any(|a| a == "--tab-lab") { builder.with_html(TAB_LAB) } else { builder.with_url(URL) };
        match builder.build_as_child(&s.slot) {
            Ok(web) => {
                let web = std::rc::Rc::new(web);
                #[cfg(target_os = "ios")]
                {
                    let page = std::rc::Rc::downgrade(&web);
                    s.slot.on_tab(move |back| {
                        if let Some(web) = page.upgrade() { let _ = web.evaluate_script(if back { "__runeTab(true)" } else { "__runeTab(false)" }); }
                    });
                }
                s.web = Some(web);
            }
            Err(e) => eprintln!("native-view: no web view: {e}"),
        }
    }
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    if s.web.is_some() && s.page_at.is_none() { s.page_at = Some(now); }
    if s.background && s.page_at.is_none() {
        s.page_at = Some(now);
        tx.minimize();
    }
    let Some(page_at) = s.page_at else { return false };
    let seconds = ((now - page_at).max(0.0)) as u64;
    tx.wake_after(Duration::from_secs_f64((page_at + (seconds + 1) as f64 - now).max(0.0)));
    if seconds == s.seconds { return false; }
    s.seconds = seconds;
    print_counts(s);
    true
}

fn print_counts(s: &State) {
    let Some(stats) = WINDOW.with(|w| w.get().map(WindowHandle::surface_stats)) else { return };
    let c = stats.totals;
    println!(
        "native-view: {}s views {} redraws {} presents {} skipped {} occluded {} timeouts {} outdated {} pacing {:?}",
        s.seconds, s.views.get(), c.redraws, c.presents, c.skipped, c.occluded, c.timeouts, c.outdated, stats.pacing,
    );
}

fn background() -> bool { std::env::var_os("NATIVE_VIEW_BACKGROUND").is_some() }

fn main() {
    #[cfg(target_os = "macos")]
    if let Some(pid) = ax_check::reader_pid() { return ax_check::run(pid); }
    let app = App::new(AppSpec::<State> {
        poll: Some(poll),
        scenes: &[("collapsed", |s| s.open = false), ("tall", |s| s.tall = true)],
        ..AppSpec::new("Native view", (900.0, 640.0), State::default, view)
    });
    let app = if background() { app.open_inactive() } else { app };
    app.on_ready(|window| {
        WINDOW.with(|w| { w.set(window.clone()).ok(); });
        if std::env::var_os("NATIVE_VIEW_CALL").is_some() { call::open(window); }
        #[cfg(target_os = "macos")]
        if background() { ax_check::spawn(); }
    })
    .run();
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime, SizeClasses};

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

    #[test]
    fn on_a_phone_the_controls_sit_below_the_status_bar_above_a_full_width_page() {
        let mut rt = Runtime::new(State::default(), view, (393.0, 852.0), Env { system_dark: false, scale: 3.0 });
        rt.set_safe_area([59.0, 0.0, 34.0, 0.0]);
        rt.set_size_classes(SizeClasses { horizontal: Some(SizeClass::Compact), vertical: Some(SizeClass::Regular) });
        rt.frame(0.0);
        let (_, title_y) = rt.find("collapse").unwrap();
        assert!(title_y > 59.0, "controls start below the status bar");
        let [x, y, w, _] = rt.native_views()[0].rect.map(|px| px / 3.0);
        assert!(y > title_y, "the page sits below the controls");
        assert!(w > 340.0 && x + w <= 393.0, "the page spans the phone's width");
    }
}
