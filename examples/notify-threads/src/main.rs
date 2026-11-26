#![forbid(unsafe_code)]

use std::time::Duration;

use rune::tok::{self, type_};
use rune::{App, AppHandle, AppSpec, Cx, El, col, el, text};
use rune_notify::{Attachment, Identity, Notification};

const APP_ID: &str = "com.example.notifythreads";
const THREAD: &str = "session-1";
const IDS: [&str; 3] = ["notify-threads-1", "notify-threads-2", "notify-threads-3"];
const PREVIEW: &[u8] = include_bytes!("../preview.png");

struct State {
    handle: AppHandle<State>,
    status: String,
}

fn three(sound: bool) -> Vec<Notification> {
    vec![
        Notification::new(IDS[0], "Build started").body("cargo build -p rune").sound(sound),
        Notification::new(IDS[1], "Screenshot ready").body("The window as it looks now"),
        Notification::new(IDS[2], "Build finished").body("All tests pass"),
    ]
    .into_iter()
    .map(|n| n.thread(THREAD, "Fix the build").info("session", THREAD))
    .collect()
}

fn preview() -> Result<Attachment, String> {
    let path = std::env::temp_dir().join("notify-threads-preview.png");
    std::fs::write(&path, PREVIEW).map_err(|e| format!("can't write the preview: {e}"))?;
    Attachment::file(&path).map_err(|e| e.to_string())
}

fn post_three(sound: bool) -> Result<String, String> {
    if !rune_notify::available() {
        return Err("This process can't post: run it as a bundled app (scripts/mac-app, scripts/ios-app).".into());
    }
    let image = preview()?;
    for (i, mut n) in three(sound).into_iter().enumerate() {
        if i == 1 {
            n = n.attachment(image.clone());
        }
        if !rune_notify::post(&n) {
            return Err(format!("The system refused notification {}.", i + 1));
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    Ok(if sound { "Posted three; the first with a sound.".into() } else { "Posted three, silent.".into() })
}

fn start(s: &mut State, sound: bool) {
    s.status = "Posting…".into();
    let handle = s.handle.clone();
    std::thread::spawn(move || {
        let status = post_three(sound).unwrap_or_else(|e| e);
        handle.update(move |s, _| s.status = status);
    });
}

fn button(key: &'static str, label: &'static str, t: &tok::Theme, press: fn(&mut State)) -> El {
    el().key(key).role("button").label(label).focusable(true)
        .w_full().padding(12.0).radius(10.0).fill(t.card)
        .on_press(press)
        .child(text(label).font(type_::BODY).color(t.text))
}

fn view(s: &State, cx: &mut Cx<State>) -> El {
    let t = tok::theme(cx.system_dark());
    let [top, right, bottom, left] = cx.safe_area();
    col().w_full().h_full().gap(12.0).pt(top + 20.0).pr(20.0 + right).pb(bottom + 20.0).pl(20.0 + left).fill(t.bg)
        .child(text("Notification threads").font(type_::TITLE).color(t.text))
        .child(button("silent", "Post three, silent", &t, |s| start(s, false)))
        .child(button("sound", "Post three, the first with a sound", &t, |s| start(s, true)))
        .child(button("clear", "Clear them", &t, |s| {
            rune_notify::remove(&IDS);
            s.status = "Cleared.".into();
        }))
        .child(text(s.status.clone()).font(type_::BODY).color(t.text_secondary).wrap().w_full())
}

fn main() {
    if let Err(e) = rune_notify::register(&Identity::new(APP_ID, "Notify threads")) {
        eprintln!("notify-threads: {e}");
    }
    let app = App::new(AppSpec::new("Notify threads", (360.0, 320.0), || unreachable!(), view));
    let handle = app.handle();
    let clicks = handle.clone();
    rune_notify::on_click(move |click| {
        let status = format!("Clicked {} in {}.", click.id, click.get("session").unwrap_or("no thread"));
        clicks.update(move |s, _| s.status = status);
    });
    app.init(move || State { handle, status: String::new() }).run();
}
