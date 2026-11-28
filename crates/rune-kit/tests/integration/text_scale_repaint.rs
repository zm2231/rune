use rune::{Cx, El, Env, Runtime, ScrollSpec, SystemMaterial, col, memo, text};
use rune_kit::shell::Toolbar;
use rune_kit::*;

#[derive(Clone, Copy, Debug)]
struct Opts {
    memo: bool,
    overlay: bool,
}

struct App {
    theme: KitTheme,
    scale: TextScale,
    o: Opts,
}

const TITLES: [&str; 12] = [
    "Planning the quarterly review with the design team",
    "Notes",
    "A session whose title runs long enough to wrap onto a second line in the list at any of these text sizes",
    "Inbox",
    "Release checklist",
    "Bug triage",
    "Planning the quarterly review with the design team, again",
    "Weekly sync",
    "Ideas",
    "Reading list",
    "Travel",
    "Groceries",
];

fn view(s: &App, cx: &mut Cx<App>) -> El {
    let o = s.o;
    cx.with_text_scale(&s.scale, |cx| {
        cx.with_kit(&s.theme, |cx| {
            let k = cx.kit();
            let bar = Toolbar::new()
                .leading(text("A session title in the toolbar that is long").key("session").font(k.ty.ui_medium).truncate().shrink().min_w(0.0))
                .title("Window")
                .material(SystemMaterial::Glass)
                .build(&k);
            let top = if o.overlay { Toolbar::height(&k) } else { 0.0 };
            let mut list = col().key("list").w_full().grow(1.0).pt(top).scroll(ScrollSpec::default());
            for (i, t) in TITLES.iter().enumerate() {
                let row = || {
                    let title = text(*t).key(format!("title{i}")).font(k.ty.body);
                    let title = if i == 2 { title.wrap() } else { title.truncate() };
                    col().key(format!("row{i}")).w_full().px(12.0).py(6.0).child(title)
                };
                list = list.child(if o.memo { memo(format!("row{i}"), k.text_scale.to_bits(), row) } else { row() });
            }
            if o.overlay {
                col().w_full().h_full().fill(k.c.bg_base).child(list).child(bar.pin(Some(0.0), Some(0.0), Some(0.0), None))
            } else {
                col().w_full().h_full().fill(k.c.bg_base).child(bar).child(list)
            }
        })
    })
}

fn runtime(scale: f32, o: Opts) -> Runtime<App> {
    let mut rt = Runtime::new(App { theme: KitTheme::neutral(), scale: TextScale::new(scale), o }, view, (360.0, 240.0), Env::new(true, 2.0));
    rt.frame(0.0);
    rt.settle_all();
    rt.frame(0.0);
    rt
}

fn paint(rt: &Runtime<App>) -> Vec<String> {
    rt.instances.iter().map(|i| format!("{:?} {:?} {:?} {:?} {:?} {:?} {:?}", i.rect, i.radii, i.fill, i.border, i.params, i.clip, i.fade)).collect()
}

fn same(a: &Runtime<App>, b: &Runtime<App>, what: &str) {
    let (a, b) = (paint(a), paint(b));
    let first = a.iter().zip(&b).position(|(x, y)| x != y);
    assert!(a.len() == b.len() && first.is_none(), "{what}: {} instances against {}, first differing {first:?}: {:?} / {:?}", a.len(), b.len(), first.map(|n| &a[n]), first.map(|n| &b[n]));
}

fn scrolled(rt: &mut Runtime<App>, by: f32) {
    if by == 0.0 { return; }
    rt.wheel(100.0, 150.0, -by);
    for _ in 0..60 { rt.frame(1.0 / 60.0); }
    rt.settle_all();
    rt.frame(1.0 / 60.0);
}

#[test]
fn stepping_the_text_scale_back_paints_what_a_fresh_window_paints() {
    for memo in [false, true] {
        for overlay in [false, true] {
            for (path, scroll) in [(&[1.6, 1.0][..], 0.0), (&[1.0, 1.6, 1.0][..], 0.0), (&[1.6, 1.0][..], 60.0), (&[1.0, 1.6, 1.0][..], 60.0)] {
                let o = Opts { memo, overlay };
                let what = format!("{o:?} path {path:?} scroll {scroll}");
                let mut rt = runtime(path[0], o);
                let top = rt.presented("row0").expect("row0").0[1];
                scrolled(&mut rt, scroll);
                if scroll > 0.0 {
                    assert!(rt.presented("row0").expect("row0").0[1] < top, "{what}: the list scrolled");
                }
                for &to in &path[1..] {
                    rt.update(move |s, _| s.scale = TextScale::new(to));
                    for _ in 0..40 { rt.frame(1.0 / 60.0); }
                }
                let mut fresh = runtime(*path.last().unwrap(), o);
                scrolled(&mut fresh, scroll);
                same(&rt, &fresh, &what);
            }
        }
    }
}

mod app_shape {
    use rune::memo::memo_with_hash;
    use rune::{Cx, El, Env, Runtime, col, row, text};
    use rune_kit::shell::{PaneState, SplitView, Toolbar};
    use rune_kit::*;

    pub(super) struct S {
        pub(super) theme: KitTheme,
        pub(super) scale: TextScale,
        pub(super) sidebar: PaneState,
    }

    fn view(s: &S, cx: &mut Cx<S>) -> El {
        cx.with_text_scale(&s.scale, |cx| {
            cx.with_kit(&s.theme, |cx| {
                let k = cx.kit();
                let side_state = s.sidebar.clone();
                memo_with_hash("frame", 7, move || {
                    let rows = (0..8).fold(col().key("sessions").w_full(), |c, i| {
                        c.child(
                            row().key(format!("session{i}")).w_full().gap(6.0).px(8.0).py(4.0).center_y()
                                .child(if i % 3 == 0 { Spinner::new().size(k.text_size(14.0)).build(&k).key(format!("spin{i}")) } else { rune::el().size(14.0, 14.0) })
                                .child(text(format!("Session {i}: a fairly long session title that truncates")).key(format!("stitle{i}")).font(k.ty.ui).truncate().shrink().min_w(0.0)),
                        )
                    });
                    let main = col().key("pane").w_full().h_full().child(text("Session header title for the open session").key("header").font(k.ty.body_medium).truncate().px(16.0).py(8.0));
                    let crumb = row().key("crumb").gap(6.0).center_y()
                        .child(rune::el().size(k.text_size(16.0), k.text_size(16.0)).radius(4.0).fill(k.c.bg_accent))
                        .child(text("A project crumb title cut to forty chars").key("crumb-title").font(k.ty.ui_medium).truncate().shrink().min_w(0.0));
                    SplitView::new(main)
                        .sidebar(&side_state, |s: &mut S| &mut s.sidebar, rows)
                        .toolbar(Toolbar::new().leading(crumb))
                        .toolbar_in_panel()
                        .titlebar_inset(28.0)
                        .frame_inset(8.0)
                        .reduced_motion(true)
                        .build(&k)
                })
            })
        })
    }

    pub(super) fn runtime(scale: f32) -> Runtime<S> {
        let s = S { theme: KitTheme::neutral(), scale: TextScale::new(scale), sidebar: PaneState::new(240.0) };
        let mut rt = Runtime::new(s, view, (900.0, 500.0), Env::new(true, 2.0));
        rt.frame(0.0);
        rt.frame(1.0 / 30.0);
        rt
    }
}

fn shape_paint(rt: &Runtime<app_shape::S>) -> Vec<String> {
    rt.instances.iter().map(|i| format!("{:?} {:?} {:?} {:?} {:?} {:?}", i.rect, i.radii, i.border, i.params[0..3].to_vec(), i.clip, i.fade)).collect()
}

#[test]
fn an_app_shaped_window_memoized_whole_paints_a_new_text_scale_as_a_fresh_one() {
    for path in [&[1.6, 1.0][..], &[1.0, 1.6, 1.0][..]] {
        let mut rt = app_shape::runtime(path[0]);
        for _ in 0..10 { rt.frame(1.0 / 30.0); }
        for &to in &path[1..] {
            rt.update(move |s, _| s.scale = TextScale::new(to));
            for _ in 0..20 { rt.frame(1.0 / 30.0); }
        }
        let mut fresh = app_shape::runtime(*path.last().unwrap());
        for _ in 0..(10 + 20 * (path.len() - 1)) { fresh.frame(1.0 / 30.0); }
        let (a, b) = (shape_paint(&rt), shape_paint(&fresh));
        let first = a.iter().zip(&b).position(|(x, y)| x != y);
        assert!(a.len() == b.len() && first.is_none(), "{path:?}: {} instances against {}, first differing {first:?}: {:?} / {:?}", a.len(), b.len(), first.map(|n| &a[n]), first.map(|n| &b[n]));
    }
}
