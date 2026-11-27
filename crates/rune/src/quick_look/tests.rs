use std::path::PathBuf;

use crate::{Cx, El, Env, QuickLook, Runtime, Tx, el};

#[derive(Default)]
struct App {
    files: Vec<PathBuf>,
    seen_open: Vec<bool>,
}

fn view(_: &App, cx: &mut Cx<App>) -> El {
    let shown = el().key(if cx.quick_look_open() { "open" } else { "closed" }).size(4.0, 4.0);
    el().key("toggle").size(80.0, 20.0).child(shown).on_press_tx(|s: &mut App, tx: &mut Tx| {
        s.seen_open.push(tx.quick_look_open());
        if tx.quick_look_open() {
            tx.close_quick_look();
        } else {
            tx.quick_look(s.files.iter().fold(QuickLook::new(), |look, f| look.file(f)).at(1));
        }
    })
}

fn app(files: &[&str]) -> Runtime<App> {
    let mut rt = Runtime::new(App { files: files.iter().map(PathBuf::from).collect(), ..Default::default() }, view, (200.0, 100.0), Env::new(true, 2.0));
    rt.frame(0.0);
    rt
}

fn press(rt: &mut Runtime<App>, t: f64) {
    rt.pointer_down(10.0, 10.0);
    rt.pointer_up(10.0, 10.0);
    rt.frame(t);
}

fn shows(rt: &Runtime<App>, key: &str) -> bool { rt.find(key).is_some() }

#[test]
fn space_bar_style_toggle_opens_then_closes_and_the_view_follows() {
    let mut rt = app(&["a.pdf", "b.png"]);
    assert!(shows(&rt, "closed"));
    press(&mut rt, 1.0 / 60.0);
    let look = rt.quick_look().unwrap_or_else(|| panic!("open; pressed {:?}", rt.state.seen_open));
    assert_eq!(look.files(), [PathBuf::from("a.pdf"), PathBuf::from("b.png")]);
    assert_eq!(look.index(), 1);
    assert!(shows(&rt, "open"));

    press(&mut rt, 2.0 / 60.0);
    assert!(rt.quick_look().is_none());
    assert!(shows(&rt, "closed"));
    assert_eq!(rt.state.seen_open, [false, true]);
}

#[test]
fn the_user_closing_the_panel_reruns_the_view_and_nothing_to_show_stays_closed() {
    let mut rt = app(&["a.pdf"]);
    press(&mut rt, 1.0 / 60.0);
    rt.close_quick_look();
    rt.frame(2.0 / 60.0);
    assert!(shows(&rt, "closed"));

    let mut rt = app(&[]);
    press(&mut rt, 1.0 / 60.0);
    assert!(rt.quick_look().is_none());
    assert!(shows(&rt, "closed"));
}

#[test]
fn a_runtime_that_never_asked_has_nothing_for_its_host() {
    let mut rt = app(&["a.pdf"]);
    assert_eq!(rt.core_mut().take_quick_look(), None);
    press(&mut rt, 1.0 / 60.0);
    assert!(rt.core_mut().take_quick_look().is_some_and(|look| look.is_some()));
    assert_eq!(rt.core_mut().take_quick_look(), None, "taken once");
}

#[test]
fn the_panel_closing_itself_wakes_the_runtime_and_the_view_reads_closed() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let mut rt = app(&["a.pdf"]);
    press(&mut rt, 1.0 / 60.0);
    assert!(rt.core_mut().take_quick_look().is_some_and(|look| look.is_some()), "the host shows it");
    let wakes = Arc::new(AtomicUsize::new(0));
    let counted = wakes.clone();
    let on_closed = rt.core().quick_look_closer(Arc::new(move || { counted.fetch_add(1, Ordering::Relaxed); }));
    assert!(shows(&rt, "open"));
    on_closed();
    assert_eq!(wakes.load(Ordering::Relaxed), 1, "a closing panel asks for a frame");
    rt.frame(2.0 / 60.0);
    assert!(shows(&rt, "closed"));
    assert!(rt.quick_look().is_none());
    assert_eq!(rt.core_mut().take_quick_look(), None, "nothing left for the host to close");
}
