#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use rune::tok::{self, radius, space, type_};
use rune::{App, AppSpec, Cx, El, Key, KeyEvent, QuickLook, ServiceInput, Tx, col, el, text};

#[derive(Default)]
struct State {
    files: Vec<PathBuf>,
    selected: usize,
    received: Vec<String>,
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

fn on_key(s: &mut State, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let moved = match ev.key {
        Key::Up => s.selected.saturating_sub(1),
        Key::Down => (s.selected + 1).min(s.files.len().saturating_sub(1)),
        Key::Text(ref t) if t == " " => {
            if tx.quick_look_open() { tx.close_quick_look(); } else { tx.quick_look(preview(s)); }
            return true;
        }
        _ => return false,
    };
    s.selected = moved;
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
    let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
    el().key(format!("file-{i}")).w_full().padding(space::S2).radius(radius::SM)
        .fill(if selected { tok::accent(0, dark) } else { t.bg })
        .on_press(move |s: &mut State| s.selected = i)
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
        .child(text("Received from Services").font(type_::SECTION).color(t.text))
        .children(s.received.iter().rev().take(5).map(|line| text(line.clone()).font(type_::LABEL).color(t.text_secondary)))
}

fn main() {
    let spec = AppSpec::<State> {
        on_key: Some(on_key),
        ..AppSpec::new("Mac Parity", (420.0, 360.0), || State { files: sample_files(), ..State::default() }, view)
    };
    App::new(spec).on_service(on_service).run();
}
