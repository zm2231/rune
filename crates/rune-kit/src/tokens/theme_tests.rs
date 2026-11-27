use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use rune::{Color, Cx, El, Env, Runtime, el, memo};

use super::*;

const RED: Color = Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
const GREEN: Color = Color { r: 0.0, g: 1.0, b: 0.0, a: 1.0 };
const BLUE: Color = Color { r: 0.0, g: 0.0, b: 1.0, a: 1.0 };

fn tinted(c: Color) -> KitTheme {
    KitTheme::new(move |env| {
        let mut k = Kit::neutral(env.dark);
        k.c.text_base = c;
        k
    })
}

struct App {
    main: KitTheme,
    chrome: KitTheme,
    seen: Rc<RefCell<Vec<(Color, bool)>>>,
    kept: Rc<RefCell<Option<Weak<Kit>>>>,
    builds: Rc<Cell<u32>>,
}

impl App {
    fn new() -> Self {
        Self { main: tinted(RED), chrome: tinted(BLUE), seen: Rc::default(), kept: Rc::default(), builds: Rc::default() }
    }
}

fn view(s: &App, cx: &mut Cx<App>) -> El {
    let note = |k: &Kit| s.seen.borrow_mut().push((k.c.text_base, k.dark));
    note(&cx.kit());
    cx.with_kit(&s.main, |cx| {
        let k = cx.kit();
        note(&k);
        *s.kept.borrow_mut() = Some(Rc::downgrade(&k));
        let builds = s.builds.clone();
        let body = memo("body", 0, move || {
            builds.set(builds.get() + 1);
            el()
        });
        let side = cx.with_kit(&s.chrome, |cx| {
            note(&cx.kit());
            el()
        });
        note(&cx.kit());
        el().child(body).child(side)
    })
}

struct Probe {
    seen: Rc<RefCell<Vec<(Color, bool)>>>,
    kept: Rc<RefCell<Option<Weak<Kit>>>>,
    builds: Rc<Cell<u32>>,
}

fn runtime() -> (Runtime<App>, Probe) {
    let app = App::new();
    let probe = Probe { seen: app.seen.clone(), kept: app.kept.clone(), builds: app.builds.clone() };
    (Runtime::new(app, view, (200.0, 100.0), Env::new(true, 1.0)), probe)
}

#[test]
fn views_read_the_innermost_theme_and_the_neutral_kit_outside_any() {
    let (mut rt, p) = runtime();
    rt.frame(0.0);
    let seen = p.seen.borrow().clone();
    let neutral = Kit::neutral(true).c.text_base;
    assert_eq!(seen, vec![(neutral, true), (RED, true), (BLUE, true), (RED, true)]);
}

#[test]
fn kits_follow_the_window_appearance() {
    let (mut rt, p) = runtime();
    rt.frame(0.0);
    p.seen.borrow_mut().clear();
    rt.set_system_dark(false);
    rt.frame(1.0 / 60.0);
    let seen = p.seen.borrow().clone();
    assert!(seen.iter().all(|&(_, dark)| !dark), "{seen:?}");
}

#[test]
fn replacing_a_theme_frees_its_kits_and_rebuilds_memos_built_under_it() {
    let (mut rt, p) = runtime();
    rt.frame(0.0);
    rt.frame(1.0 / 60.0);
    assert_eq!(p.builds.get(), 1, "an unchanged theme reuses the memo");
    let old = p.kept.borrow().clone().expect("kit");
    rt.update(|s, _| s.main = tinted(GREEN));
    rt.frame(2.0 / 60.0);
    assert_eq!(p.builds.get(), 2, "a new theme rebuilds the memo");
    assert!(old.upgrade().is_none(), "the replaced theme's kit is freed");
}

#[test]
fn a_theme_keeps_kits_for_its_four_latest_environments() {
    let builds = Rc::new(Cell::new(0));
    let counted = builds.clone();
    let theme = KitTheme::new(move |env| {
        counted.set(counted.get() + 1);
        Kit::neutral(env.dark)
    });
    let env = |i: u8| KitEnv::new(true).accent(Color { r: i as f32 / 8.0, ..RED });
    let first = theme.kit(&env(0));
    for i in 1..4 {
        theme.kit(&env(i));
    }
    assert!(Rc::ptr_eq(&first, &theme.kit(&env(0))), "four environments fit");
    assert_eq!(builds.get(), 4);
    theme.kit(&env(4));
    theme.kit(&env(1));
    assert_eq!(builds.get(), 6, "the least recently used environment was dropped");
    let weak = Rc::downgrade(&first);
    drop(first);
    drop(theme);
    assert!(weak.upgrade().is_none(), "dropping the theme frees its kits");
}
