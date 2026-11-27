#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use rune::tok::{self, type_};
use rune::{
    Activity, AppSpec, Cx, El, FileDialogOptions, MediaKind, MediaPicker, ScrollSpec, SearchIndexError, SearchItem, Share, ShareError, SizeClass, TextEdit, Tx, col, el, row, text,
    text_input,
};

#[derive(Default)]
struct State {
    status: String,
    picked: Vec<PathBuf>,
    pasted: String,
    scratch: TextEdit,
    shares: u32,
    sharing: Option<u32>,
}

impl State {
    fn say(&mut self, status: impl Into<String>) { self.status = status.into(); }
}

fn share(s: &mut State, tx: &mut Tx, what: &'static str, share: Share, anchor: &'static str) {
    s.shares += 1;
    let n = s.shares;
    match s.sharing {
        Some(open) => s.say(format!("{what} #{n}: asked while #{open} is open")),
        None => s.say(format!("{what} #{n}: sharing…")),
    }
    s.sharing = s.sharing.or(Some(n));
    tx.share(share, anchor, move |s: &mut State, result, _| match result {
        Err(ShareError::Busy) => s.say(format!("{what} #{n}: refused, #{} is still open", s.sharing.unwrap_or(n))),
        result => {
            if s.sharing == Some(n) { s.sharing = None; }
            s.say(format!("{what} #{n}: {result:?}"));
        }
    });
}

fn names(paths: &[PathBuf]) -> String {
    paths.iter().map(|p| p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned())).collect::<Vec<_>>().join(", ")
}

fn button(key: &'static str, label: &'static str, t: &tok::Theme, on_press: impl Fn(&mut State, &mut Tx) + 'static) -> El {
    el().key(key).role("button").label(label).min_h(44.0).px(16.0).py(10.0).radius(12.0).fill(t.card)
        .child(text(label).font(type_::BODY).color(t.text))
        .on_press_tx(move |s: &mut State, tx: &mut Tx| on_press(s, tx))
}

fn section(title: &'static str, t: &tok::Theme, buttons: Vec<El>) -> El {
    buttons.into_iter().fold(col().w_full().gap(8.0).child(text(title).font(type_::SECTION).color(t.text)), El::child)
}

fn links(t: &tok::Theme) -> El {
    section("Links", t, vec![
        button("open-web", "Open example.com", t, |s, tx| {
            if let Err(e) = tx.open_url_then("https://example.com/", |s: &mut State, result, _| s.say(format!("example.com: {result:?}"))) {
                s.say(format!("example.com: {e}"));
            }
        }),
        button("open-mail", "Open a mailto: link", t, |s, tx| {
            if let Err(e) = tx.open_url_then("mailto:hello@example.com", |s: &mut State, result, _| s.say(format!("mailto: {result:?}"))) {
                s.say(format!("mailto: {e}"));
            }
        }),
    ])
}

fn clipboard(state: &State, t: &tok::Theme) -> El {
    section("Clipboard", t, vec![
        button("copy", "Copy \"Hello from Rune\"", t, |s, tx| {
            tx.copy("Hello from Rune");
            s.say("Copied");
        }),
        button("paste", "Read the clipboard", t, |_, tx| {
            tx.read_clipboard(|s: &mut State, content, _| {
                let image = content.image().map(|i| format!(", image {}x{}", i.size().0, i.size().1)).unwrap_or_default();
                s.pasted = format!("text {:?}{image}", content.text());
                s.say("Read the clipboard");
            });
        }),
        text(format!("Clipboard: {}", state.pasted)).key("pasted").font(type_::LABEL).color(t.text_secondary).wrap().w_full(),
        text_input(&state.scratch, type_::BODY).key("scratch").label("Paste here").placeholder("Paste here", t.text_secondary)
            .color(t.text).w_full().min_h(44.0).padding(12.0).radius(12.0).fill(t.card).on_edit(|s: &mut State| &mut s.scratch),
    ])
}

fn sharing(t: &tok::Theme) -> El {
    section("Share", t, vec![
        button("share-link", "Share a link", t, |s, tx| {
            share(s, tx, "Share link", Share::new().title("Rune").text("Made with Rune").url("https://example.com/"), "share-link");
        }),
        button("share-picked", "Share the picked files", t, |s, tx| {
            let files = s.picked.iter().fold(Share::new(), |share, path| share.file(path.clone()));
            share(s, tx, "Share files", files, "share-picked");
        }),
    ])
}

fn pickers(state: &State, t: &tok::Theme) -> El {
    let answered = |what: &'static str| move |s: &mut State, result: Result<Option<Vec<PathBuf>>, rune::FileDialogError>, _: &mut Tx| match result {
        Ok(Some(paths)) => {
            s.say(format!("{what}: {} picked", paths.len()));
            s.picked = paths;
        }
        Ok(None) => s.say(format!("{what}: cancelled")),
        Err(e) => s.say(format!("{what}: {e}")),
    };
    section("Photos and files", t, vec![
        button("pick-photo", "Pick a photo", t, move |_, tx| tx.pick_media(MediaPicker::default(), answered("Photo"))),
        button("pick-media", "Pick photos and videos", t, move |_, tx| {
            tx.pick_media(MediaPicker::new(MediaKind::ImagesAndVideos).limit(0), answered("Media"));
        }),
        button("import", "Import files", t, move |_, tx| tx.open_files(FileDialogOptions::default(), answered("Import"))),
        button("export", "Export a text file", t, |_, tx| {
            let options = FileDialogOptions { file_name: Some("Rune export.txt".into()), ..Default::default() };
            tx.save_file(options, |s: &mut State, result, _| match result {
                Ok(Some(path)) => {
                    s.say(format!("Exporting to {}", path.display()));
                    export(path);
                }
                Ok(None) => s.say("Export: cancelled"),
                Err(e) => s.say(format!("Export: {e}")),
            });
        }),
        text(format!("Picked: {}", names(&state.picked))).key("picked").font(type_::LABEL).color(t.text_secondary).wrap().w_full(),
    ])
}

const VIEWING: &str = "dev.rune.ios-system.viewing";
const NOTES: [&str; 3] = ["Groceries", "Packing list", "Book club"];

fn system_search(t: &tok::Theme) -> El {
    section("Handoff and Spotlight", t, vec![
        button("index", "Index three notes in Spotlight", t, |s, tx| {
            tx.index_search_items(NOTES.iter().enumerate().map(|(i, title)| {
                SearchItem::new(format!("note-{}", i + 1)).with_domain("notes").with_title(*title)
                    .with_details("A note from the Rune system example").with_keyword("rune").with_keyword("system")
            }));
            s.say("Indexed 3 notes: search for \"Groceries\" or \"rune\" from the home screen");
        }),
        button("unindex", "Remove them from Spotlight", t, |s, tx| {
            tx.remove_search_domain("notes");
            s.say("Removed the notes from Spotlight");
        }),
        button("advertise", "Advertise \"Groceries\" for Handoff", t, |s, tx| {
            tx.advertise(Activity::new(VIEWING).with_title("Viewing Groceries").with_info("note", "note-1"));
            s.say("Advertising note-1");
        }),
        button("stop-advertising", "Stop advertising", t, |s, tx| {
            tx.stop_advertising();
            s.say("Not advertising");
        }),
    ])
}

fn on_open_search_item(s: &mut State, id: &str, _: &mut Tx) { s.say(format!("Opened from Spotlight: {id}")); }

fn on_search_index_error(s: &mut State, error: &SearchIndexError, _: &mut Tx) { s.say(format!("Spotlight: {error}")); }

fn on_continue(s: &mut State, activity: &Activity, _: &mut Tx) {
    s.say(format!("Continued {}: {}", activity.kind(), activity.info("note").unwrap_or("?")));
}

fn export(path: PathBuf) {
    std::thread::spawn(move || write(&path));
}

fn write(path: &Path) {
    if let Err(e) = std::fs::write(path, "Exported from Rune\n") { eprintln!("export to {}: {e}", path.display()); }
}

fn view(state: &State, cx: &mut Cx<State>) -> El {
    let t = tok::theme(cx.system_dark());
    let [top, right, bottom, left] = cx.safe_area();
    let (horizontal, vertical) = (cx.horizontal_size_class(), cx.vertical_size_class());
    let [width, height] = cx.layout_size("root").unwrap_or_default();
    let header = col().w_full().gap(4.0)
        .child(text("System").role("heading").level(1).font(type_::TITLE).color(t.text))
        .child(text(format!("Size classes: {horizontal:?} across, {vertical:?} down; {width:.0}×{height:.0} pt")).key("size-class")
            .font(type_::LABEL).color(t.text_secondary).wrap().w_full())
        .child(text(format!("Status: {}", state.status)).key("status").font(type_::LABEL).color(t.text_secondary).wrap().w_full());
    let first = col().w_full().gap(24.0).child(links(&t)).child(clipboard(state, &t));
    let second = col().w_full().gap(24.0).child(sharing(&t)).child(pickers(state, &t)).child(system_search(&t));
    let body = if horizontal == Some(SizeClass::Regular) {
        row().w_full().gap(32.0).child(first.grow(1.0).shrink()).child(second.grow(1.0).shrink())
    } else {
        col().w_full().gap(24.0).child(first).child(second)
    };
    let page = col().w_full().gap(24.0).py(16.0).child(header).child(body);
    col().key("root").w_full().h_full().pt(top).pr(16.0 + right).pb(bottom).pl(16.0 + left).fill(t.bg)
        .child(col().key("page").w_full().grow(1.0).shrink().scroll(ScrollSpec { stick_bottom: false }).child(page))
}

fn main() {
    rune::App::new(AppSpec::new("System", (390.0, 760.0), State::default, view))
        .on_open_search_item(on_open_search_item)
        .on_search_index_error(on_search_index_error)
        .on_continue_activity(on_continue)
        .run();
}
