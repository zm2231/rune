use std::cell::{Cell, OnceCell};
use std::time::Duration;

use rune::tok::{self, type_};
use rune::{AppSpec, Cx, El, NativeView, Tx, WindowHandle, WindowOptions, WindowSpec, col, el, text};

const PAGE_AFTER: f64 = 1.5;
const TICK: Duration = Duration::from_millis(100);

struct Call {
    slot: NativeView,
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    web: Option<wry::WebView>,
    opened_at: Option<f64>,
    shown: bool,
    page_at: Option<f64>,
    seconds: u64,
    views: Cell<u64>,
}

impl Default for Call {
    fn default() -> Self {
        Call {
            slot: NativeView::new(),
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            web: None,
            opened_at: None,
            shown: false,
            page_at: None,
            seconds: 0,
            views: Cell::new(0),
        }
    }
}

thread_local! {
    static CALL: OnceCell<WindowHandle> = const { OnceCell::new() };
    static MAIN: OnceCell<WindowHandle> = const { OnceCell::new() };
}

pub fn open(main: &WindowHandle) {
    MAIN.with(|w| { w.set(main.clone()).ok(); });
    let spec = WindowSpec::new("call", AppSpec::<Call> {
        poll: Some(poll),
        ..AppSpec::new("Call", (360.0, 240.0), Call::default, view)
    })
    .options(WindowOptions { transparent: true, ..Default::default() });
    let handle = rune::open_window(spec);
    CALL.with(|w| { w.set(handle).ok(); });
}

fn view(s: &Call, cx: &mut Cx<Call>) -> El {
    s.views.set(s.views.get() + 1);
    let t = tok::theme(cx.system_dark());
    let label = if s.page_at.is_some() { format!("Page up for {} s", s.seconds) } else { "No page yet".into() };
    let body = col().key("call").w_full().h_full().gap(10.0).padding(16.0).fill(t.card)
        .child(text("Call").font(type_::TITLE).color(t.text))
        .child(text(label).key("seconds").font(type_::BODY).color(t.text_secondary));
    if s.shown {
        body.child(el().key("hidden-page").size(1.0, 1.0).opacity(0.01).native_view(&s.slot))
    } else {
        body
    }
}

fn poll(s: &mut Call, now: f64, tx: &mut Tx) -> bool {
    tx.wake_after(TICK);
    let opened_at = *s.opened_at.get_or_insert(now);
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    if s.web.is_none() && s.slot.in_window() {
        match wry::WebViewBuilder::new().with_url(super::URL).build_as_child(&s.slot) {
            Ok(web) => { s.web = Some(web); s.page_at = Some(now); return true; }
            Err(e) => eprintln!("native-view: no web view: {e}"),
        }
    }
    if !s.shown && now - opened_at >= PAGE_AFTER { s.shown = true; return true; }
    let Some(page_at) = s.page_at else { return false };
    let seconds = ((now - page_at).max(0.0)) as u64;
    if seconds == s.seconds { return false; }
    s.seconds = seconds;
    print_counts(s);
    true
}

fn print_counts(s: &Call) {
    let line = |w: &'static std::thread::LocalKey<OnceCell<WindowHandle>>| {
        w.with(|w| w.get().map(WindowHandle::surface_stats)).map(|stats| {
            let c = stats.totals;
            format!(
                "redraws {} presents {} skipped {} occluded {} timeouts {} outdated {} pacing {:?}",
                c.redraws, c.presents, c.skipped, c.occluded, c.timeouts, c.outdated, stats.pacing,
            )
        }).unwrap_or_default()
    };
    println!("native-view: call {}s views {} {}", s.seconds, s.views.get(), line(&CALL));
    println!("native-view: main {}s {}", s.seconds, line(&MAIN));
}
