#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use rune::tok::{self, radius, space, type_};
use rune::{Activity, App, AppSpec, Cx, El, Key, KeyEvent, QuickLook, SearchIndexError, SearchItem, ServiceInput, Tx, col, el, text};

const VIEWING: &str = "dev.rune.mac-parity.viewing";

#[derive(Default)]
struct State {
    files: Vec<PathBuf>,
    selected: usize,
    received: Vec<String>,
    continued: Option<String>,
    search: String,
}

fn sample_files() -> Vec<PathBuf> {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).filter(|p| p.is_file()).collect();
    if !args.is_empty() { return args; }
    let dir = std::env::temp_dir().join("rune-mac-parity");
    let samples = [
        ("notes.md", "# Notes\n\nPress space to preview, arrows to move.\n"),
        ("todo.txt", "Quick Look\nServices\nHandoff\nSpotlight\n"),
        ("data.csv", "name,count\nalpha,1\nbeta,2\n"),
    ];
    if std::fs::create_dir_all(&dir).is_err() { return Vec::new(); }
    samples.iter().filter_map(|(name, body)| {
        let path = dir.join(name);
        std::fs::write(&path, body).ok().map(|()| path)
    }).collect()
}

fn preview(s: &State) -> QuickLook {
    s.files.iter().fold(QuickLook::new(), |look, f| look.file(f)).at(s.selected)
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

fn select(s: &mut State, i: usize, tx: &mut Tx) {
    s.selected = i;
    if let Some(path) = s.files.get(i) {
        tx.advertise(Activity::new(VIEWING).with_title(format!("Viewing {}", file_name(path))).with_info("file", file_name(path)));
    }
}

fn on_continue(s: &mut State, activity: &Activity, tx: &mut Tx) {
    if activity.kind() != VIEWING { return; }
    let Some(name) = activity.info("file") else { return };
    s.continued = Some(name.to_owned());
    if let Some(i) = s.files.iter().position(|f| file_name(f) == name) { select(s, i, tx); }
}

fn index_files(s: &mut State, tx: &mut Tx) {
    tx.index_search_items(s.files.iter().map(|f| {
        SearchItem::new(file_name(f)).with_domain("files").with_title(file_name(f)).with_details(f.display().to_string()).with_keyword("mac").with_keyword("parity")
    }));
    s.search = format!("Indexed {} files; search Spotlight for \"parity\" or a file's name", s.files.len());
}

fn on_open_search_item(s: &mut State, id: &str, tx: &mut Tx) {
    s.search = format!("Opened from Spotlight: {id}");
    if let Some(i) = s.files.iter().position(|f| file_name(f) == id) { select(s, i, tx); }
}

fn on_search_index_error(s: &mut State, error: &SearchIndexError, _: &mut Tx) { s.search = error.to_string(); }

fn on_key(s: &mut State, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let moved = match ev.key {
        Key::Up => s.selected.saturating_sub(1),
        Key::Down => (s.selected + 1).min(s.files.len().saturating_sub(1)),
        Key::Text(ref t) if t == "i" => {
            index_files(s, tx);
            return true;
        }
        Key::Text(ref t) if t == "r" => {
            tx.remove_search_domain("files");
            s.search = "Removed the files from Spotlight".into();
            return true;
        }
        Key::Text(ref t) if t == " " => {
            if tx.quick_look_open() { tx.close_quick_look(); } else { tx.quick_look(preview(s)); }
            return true;
        }
        _ => return false,
    };
    select(s, moved, tx);
    if tx.quick_look_open() { tx.quick_look(preview(s)); }
    true
}

fn on_service(s: &mut State, input: &ServiceInput, _: &mut Tx) {
    let mut parts: Vec<String> = input.text().map(|t| format!("text {t:?}")).into_iter().collect();
    parts.extend(input.urls().iter().map(|u| format!("link {u}")));
    parts.extend(input.files().iter().map(|f| format!("file {}", f.display())));
    s.received.push(format!("{}: {}", input.name(), parts.join(", ")));
    for file in input.files().iter().filter(|f| f.is_file()) {
        if !s.files.contains(file) { s.files.push(file.clone()); }
    }
}

fn row_for(i: usize, path: &Path, selected: bool, dark: bool) -> El {
    let t = tok::theme(dark);
    let name = file_name(path);
    el().key(format!("file-{i}")).w_full().padding(space::S2).radius(radius::SM)
        .fill(if selected { tok::accent(0, dark) } else { t.bg })
        .on_press_tx(move |s: &mut State, tx: &mut Tx| select(s, i, tx))
        .child(text(name).font(type_::BODY).color(t.text))
}

fn view(s: &State, cx: &mut Cx<State>) -> El {
    let dark = cx.system_dark();
    let t = tok::theme(dark);
    let status = if cx.quick_look_open() { "Quick Look open (space closes)" } else { "Space previews the selected file" };
    let rows = s.files.iter().enumerate().map(|(i, f)| row_for(i, f, i == s.selected, dark));
    col().w_full().h_full().padding(space::S6).gap(space::S2).fill(t.bg)
        .child(text("Mac parity").font(type_::TITLE).color(t.text))
        .child(text(status).key("status").font(type_::LABEL).color(t.text_secondary))
        .children(rows)
        .child(text(match &s.continued { Some(name) => format!("Continued from another device: {name}"), None => "Handoff advertises the selected file".into() }).key("handoff").font(type_::LABEL).color(t.text_secondary))
        .child(text(if s.search.is_empty() { "I indexes the files in Spotlight, R removes them" } else { &s.search }).key("search").font(type_::LABEL).color(t.text_secondary))
        .child(text("Received from Services").font(type_::SECTION).color(t.text))
        .children(s.received.iter().rev().take(5).map(|line| text(line.clone()).font(type_::LABEL).color(t.text_secondary)))
}

fn main() {
    let spec = AppSpec::<State> {
        on_key: Some(on_key),
        ..AppSpec::new("Mac Parity", (420.0, 360.0), || State { files: sample_files(), ..State::default() }, view)
    };
    App::new(spec).on_service(on_service).on_continue_activity(on_continue)
        .on_open_search_item(on_open_search_item).on_search_index_error(on_search_index_error).run();
}
