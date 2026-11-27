use rune::{Cx, El, Env, Key, KeyEvent, Mods, Runtime, col, el, row};

use super::*;
use crate::{Badge, Button, Checkbox, Field, Keybind, KitCx, KitTheme, RadioItem, SegmentedControl, SidebarRow};

struct App {
    theme: KitTheme,
    scale: TextScale,
}

fn view(s: &App, cx: &mut Cx<App>) -> El {
    cx.with_text_scale(&s.scale, |cx| {
        cx.with_kit(&s.theme, |cx| {
            let k = cx.kit();
            col()
                .child(row().child(Button::new("Save").build(&k).key("button")))
                .child(el().key("box").size(120.0, 40.0).px(12.0))
        })
    })
}

fn sizes(scale: f32) -> ([f32; 4], [f32; 4]) {
    let app = App { theme: KitTheme::neutral(), scale: TextScale::new(scale) };
    let mut rt = Runtime::new(app, view, (400.0, 300.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.settle_all();
    rt.frame(0.0);
    (rt.presented("button").expect("button").0, rt.presented("box").expect("box").0)
}

#[test]
fn a_text_scale_grows_text_controls_and_leaves_the_rest_of_the_layout() {
    let (button, plain) = sizes(1.0);
    let (bigger, same) = sizes(1.5);
    assert_eq!(bigger[3], button[3] * 1.5, "the button's height holds its larger label");
    assert!(bigger[2] > button[2], "its label is wider");
    assert_eq!(same[2..], plain[2..], "a box with no text keeps its size");
}

#[test]
fn a_kit_scales_its_type_and_text_metrics_from_the_themes_own_size() {
    let k = Kit::neutral(true);
    let big = k.with_text_scale(1.5);
    assert_eq!(big.text_scale, 1.5);
    assert_eq!(big.ty.ui.size, k.ty.ui.size * 1.5);
    assert_eq!(big.ty.body.line_height, k.ty.body.line_height * 1.5);
    assert_eq!(big.radius.md, k.radius.md, "radii stay");
    let button = big.button_metrics(crate::ControlSize::Regular);
    let base = k.button_metrics(crate::ControlSize::Regular);
    assert_eq!((button.height, button.icon, button.padding_x), (base.height * 1.5, base.icon * 1.5, base.padding_x));
    assert_eq!(big.text_size(28.0), 42.0);
    assert_eq!(big.text(rune::TextStyle::new(12.0, 400.0)).size, 18.0);
    let back = big.with_text_scale(1.0);
    assert_eq!(back.ty.ui, k.ty.ui, "scales are from the theme's size, not compounded");
    assert_eq!(back.button_metrics(crate::ControlSize::Regular), base);
    assert_eq!(k.with_text_scale(f32::NAN).text_scale, 1.0);
    assert_eq!(k.with_text_scale(10.0).text_scale, TEXT_SCALES.1);
}

#[test]
fn steps_walk_the_standard_sizes_and_stop_at_the_ends() {
    let s = TextScale::default();
    assert_eq!(s.step(TextStep::Bigger).get(), 1.15);
    assert_eq!(s.step(TextStep::Smaller).get(), 0.85);
    assert_eq!(TextScale::new(1.2).step(TextStep::Bigger).get(), 1.3);
    assert_eq!(TextScale::new(1.2).step(TextStep::Smaller).get(), 1.15);
    assert_eq!(TextScale::new(2.0).step(TextStep::Normal).get(), 1.0);
    let top = TextScale::new(2.5);
    assert_eq!(top.step(TextStep::Bigger).0.id(), top.0.id(), "no step past the largest");
    assert_eq!(TextScale::new(0.75).step(TextStep::Smaller).get(), 0.75);
}

#[test]
fn command_equals_minus_and_zero_are_the_text_size_keys() {
    let cmd = Mods { cmd: true, ..Mods::default() };
    let ev = |c| KeyEvent { key: Key::Command(c), mods: cmd };
    assert_eq!(TextStep::from_key(&ev('=')), Some(TextStep::Bigger));
    assert_eq!(TextStep::from_key(&ev('+')), Some(TextStep::Bigger));
    assert_eq!(TextStep::from_key(&ev('-')), Some(TextStep::Smaller));
    assert_eq!(TextStep::from_key(&ev('0')), Some(TextStep::Normal));
    assert_eq!(TextStep::from_key(&ev('s')), None);
    assert_eq!(TextStep::from_key(&KeyEvent { key: Key::Text("=".into()), mods: Mods::default() }), None);
    let _ = TextScale::menu_items::<App>(|s| &mut s.scale);
}

const CONTROLS: [&str; 7] = ["badge", "keybind", "segmented", "checkbox", "radio", "field", "sidebar_row"];

struct Controls(TextScale);

fn controls(s: &Controls, cx: &mut Cx<Controls>) -> El {
    let theme = KitTheme::neutral();
    cx.with_text_scale(&s.0, |cx| {
        cx.with_kit(&theme, |cx| {
            let k = cx.kit();
            let one = |e: El, key: &str| row().child(e.key(key));
            col()
                .items_start()
                .child(one(Badge::new("New").build(&k), "badge"))
                .child(one(Keybind::new(&["cmd", "K"]).build(&k), "keybind"))
                .child(one(SegmentedControl::new(&["Day", "Week"], Some(0)).build(&k), "segmented"))
                .child(one(Checkbox::new(true).label("Wrap lines").build(&k), "checkbox"))
                .child(one(RadioItem::new("Compact", true).build(&k), "radio"))
                .child(one(Field::new().label("Name").info().prefix("Shown to others").build(&k), "field"))
                .child(el().w(240.0).child(SidebarRow::new("Inbox").build(&k).key("sidebar_row")))
        })
    })
}

fn rendered(scale: f32) -> Vec<rune::ElementInfo> {
    let mut rt = Runtime::new(Controls(TextScale::new(scale)), controls, (600.0, 900.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.settle_all();
    rt.frame(0.0);
    rt.elements()
}

fn control<'a>(els: &'a [rune::ElementInfo], key: &str) -> &'a rune::ElementInfo {
    els.iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key} is rendered"))
}

fn line_boxes<'a>(els: &'a [rune::ElementInfo], of: &'a rune::ElementInfo) -> impl Iterator<Item = &'a rune::ElementInfo> {
    els.iter().filter(move |e| e.text.is_some() && e.path.starts_with(&of.path) && e.path[of.path.len()..].starts_with('/'))
}

#[test]
fn at_twice_the_text_size_every_control_holds_its_text_and_grows_only_by_it() {
    let (plain, big) = (rendered(1.0), rendered(2.0));
    for key in CONTROLS {
        let (a, b) = (control(&plain, key), control(&big, key));
        let [x, y, w, h] = b.rect;
        let mut texts = 0;
        for t in line_boxes(&big, b) {
            texts += 1;
            let [tx, ty, tw, th] = t.rect;
            let line = t.text_size.expect("laid-out text")[1];
            assert!(line <= th + 0.5, "{key}: {:?}'s line box ({line}) fits its element ({th})", t.text);
            assert!(tx >= x - 0.5 && ty >= y - 0.5 && tx + tw <= x + w + 0.5 && ty + th <= y + h + 0.5, "{key}: {:?} at {:?} lies inside {:?}", t.text, t.rect, b.rect);
            let size = t.text_style.as_ref().expect("text style").size;
            let before = line_boxes(&plain, a).find(|p| p.text == t.text).expect("the same text at 1x");
            assert_eq!(size, before.text_style.as_ref().unwrap().size * 2.0, "{key}: {:?} doubles", t.text);
        }
        assert!(texts > 0, "{key} shows text");
        assert!(h > a.rect[3] * 1.5, "{key}'s height ({} to {h}) follows its text", a.rect[3]);
        assert_eq!(b.radius, a.radius, "{key}'s radius stays");
        assert_eq!(b.inset, a.inset, "{key}'s padding stays");
    }
}

#[cfg(feature = "markdown")]
const NOTES: &str = "```facts\nLatency: {verified} 11.9 ms\n```\n\n```compare\nBefore: Software loop.\nAfter: One GPU pass.\n```\n\n```steps\n{built} Shared pass\n{proposed} Live preview\n```\n\n```size\napp +120 \u{2212}30 \u{b7} tests +45 \u{2212}0 \u{b7} 4 files\n```\n";

struct Scene {
    scale: TextScale,
    diff: crate::diff::DiffState,
}

fn scene(s: &Scene, cx: &mut Cx<Scene>) -> El {
    use crate::glyphs::STAR;
    use crate::shell::{TabItem, TabOrientation, TabStrip};
    use crate::{List, ListItem, Menu, MenuEntry, MenuItem, Select, SidebarHeader, Toast, TreeItem, TreeRow, TreeStyle};
    let theme = KitTheme::neutral();
    cx.with_text_scale(&s.scale, |cx| {
        cx.with_kit(&theme, |cx| {
            let k = cx.kit();
            let k = &*k;
            let mut layer = Vec::new();
            let menu = Menu::new("menu")
                .trigger(Button::new("Edit").build(k))
                .item(MenuItem::new("Copy").icon(&STAR))
                .item(MenuItem::new("Wrap").checked(true))
                .item(MenuItem::new("More").submenu(vec![MenuEntry::item(MenuItem::new("Less"))], false))
                .open(true)
                .build(k, &mut layer);
            let select = Select::new("select", &["One", "Two"]).current(Some("One")).open(true).build(k, &mut layer);
            let tabs = TabStrip::new("tabs", TabOrientation::Horizontal)
                .tabs([TabItem::new("a", "Alpha").icon(&STAR).closable(true), TabItem::new("b", "Beta").status(Some(crate::shell::TabItemStatus::Busy))])
                .viewport_width(Some(400.0))
                .on_close(|_: &mut Scene, _, _| {})
                .build(k);
            let list = List::new().item(ListItem::new("Inbox").selected(true)).build(k);
            let tree = TreeItem::new("src").icon(&STAR).build(k, &TreeRow::new("src", 0), &TreeStyle::from_kit(k));
            let folder = TreeItem::new("lib").build(k, &TreeRow::new("lib", 0).expanded(Some(true)), &TreeStyle::from_kit(k));
            let diff = crate::diff::DiffView::new(|s: &Scene| &s.diff, |s: &mut Scene| &mut s.diff).on_comment(|_: &mut Scene, _, _| {}).build(s, k);
            #[cfg(feature = "markdown")]
            let markdown = crate::Markdown::new("# One\n\n## Two\n\n### Three\n\n#### Four\n\nBody with `code`.\n\n![Missing chart](missing.png)\n\n```\nlet x = 1;\n```\n").build(k);
            #[cfg(not(feature = "markdown"))]
            let markdown = el();
            #[cfg(feature = "markdown")]
            let notes = crate::Note::new(NOTES).build(k);
            #[cfg(not(feature = "markdown"))]
            let notes = el();
            col()
                .w(900.0)
                .items_start()
                .child(el().w(600.0).child(markdown.key("markdown")))
                .child(el().w(600.0).child(notes.key("notes")))
                .child(row().child(tabs.key("tab_strip")))
                .child(row().child(list.key("list")))
                .child(row().child(menu))
                .child(row().child(select))
                .child(Toast::new("toast", "Saved").icon(k, &STAR).closable().build(k).key("toast"))
                .child(SidebarHeader::new("Pinned").collapsed(false).build(k).key("sidebar_header"))
                .child(el().w(240.0).child(SidebarRow::new("Inbox").icon(&STAR).status(crate::shell::RowStatus::Busy).build(k).key("sidebar_icon_row")))
                .child(el().w(300.0).child(tree.key("tree_row")))
                .child(el().w(240.0).child(SidebarRow::new("Inbox").icon(&STAR).subtitle("3 unread").build(k).key("sidebar_two_icon")))
                .child(el().w(240.0).child(SidebarRow::new("Ada").avatar(el().size(k.text_size(16.0), k.text_size(16.0))).subtitle("Online").build(k).key("sidebar_two_avatar")))
                .child(el().w(300.0).child(folder.key("tree_folder")))
                .child(el().w(900.0).h(400.0).child(diff.key("diff")))
                .children(layer)
        })
    })
}

fn scene_at(scale: f32) -> Vec<rune::ElementInfo> {
    let old: String = (1..=40).map(|i| format!("line {i}\n")).collect();
    let new = old.replace("line 20\n", "line twenty\n");
    let diff = crate::diff::DiffState::new(vec![crate::diff::FileDiff::from_texts("a.txt", &old, &new)]);
    let mut rt = Runtime::new(Scene { scale: TextScale::new(scale), diff }, scene, (900.0, 1600.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.settle_all();
    rt.frame(0.0);
    rt.elements()
}

#[test]
fn at_twice_the_text_size_kit_text_and_the_glyphs_beside_it_double() {
    let (plain, big) = (scene_at(1.0), scene_at(2.0));
    let at = |els: &[rune::ElementInfo], path: &str| els.iter().find(|e| e.path == path).cloned();
    let (mut texts, mut glyphs, mut stuck) = (0, 0, Vec::new());
    for b in &big {
        let Some(a) = at(&plain, &b.path) else { continue };
        if let (Some(sa), Some(sb)) = (&a.text_style, &b.text_style) {
            texts += 1;
            if (sb.size, sb.line_height) != (sa.size * 2.0, sa.line_height * 2.0) {
                stuck.push(format!("{} {:?}: {} / {} to {} / {}", b.path, b.text, sa.size, sa.line_height, sb.size, sb.line_height));
            }
        }
        let holder = matches!(b.key.as_deref(), Some("status" | "status-mark" | "indicator" | "avatar" | "dot"))
            || (b.key.as_deref() == Some("icon") && b.role != "icon")
            || (b.role == "image" && b.path.contains("/markdown/"));
        let broken_glyph = big.iter().any(|p| p.role == "image" && p.path.contains("/markdown/") && b.path == format!("{}/#0", p.path));
        if b.role == "icon" || b.role == "progressbar" || (holder && b.role != "image") || broken_glyph {
            glyphs += 1;
            if [b.target[2], b.target[3]] != [a.target[2] * 2.0, a.target[3] * 2.0] {
                stuck.push(format!("{} glyph: {:?} to {:?}", b.path, &a.target[2..], &b.target[2..]));
            }
        }
    }
    assert!(stuck.is_empty(), "text and glyphs that kept their size:\n{}", stuck.join("\n"));
    for key in ["tab_strip", "list", "menu-menu", "select-content", "toast", "sidebar_header", "sidebar_icon_row", "tree_row", "tree_folder", "diff"]
        .into_iter()
        .chain(cfg!(feature = "markdown").then_some("markdown"))
        .chain(cfg!(feature = "markdown").then_some("notes")) {
        let inside = |els: &[rune::ElementInfo]| {
            let root = els.iter().find(|e| e.key.as_deref() == Some(key) || e.path.ends_with(&format!("/{key}"))).map(|e| e.path.clone());
            root.map_or(0, |r| els.iter().filter(|e| e.path.starts_with(&r) && (e.text_style.is_some() || e.role == "icon")).count())
        };
        assert!(inside(&big) > 0, "{key} draws text or glyphs");
    }
    assert!(texts > 20 && glyphs > 8, "{texts} texts and {glyphs} glyphs compared");

    let tab = |key: &str| big.iter().find(|e| e.path.contains("/t-b/") && e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("the busy tab's {key}"));
    let (title, status) = (tab("title"), tab("status"));
    assert!(title.target[0] + title.target[2] <= status.target[0] + 0.5, "the busy tab's title {:?} ends before its status {:?}", title.target, status.target);

    for els in [&plain, &big] {
        for row in ["sidebar_two_icon", "sidebar_two_avatar"] {
            let part = |key: &str| els.iter().find(|e| e.path.contains(&format!("/{row}/")) && e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{row}'s {key}"));
            let (label, subtitle) = (part("label"), part("subtitle"));
            assert!((label.target[0] - subtitle.target[0]).abs() < 0.5, "{row}'s subtitle {:?} starts under its label {:?}", subtitle.target, label.target);
        }
    }

    let mut pluses = 0;
    for b in big.iter().filter(|e| e.key.as_deref() == Some("plus")) {
        let a = at(&plain, &b.path).expect("the same comment button at 1x");
        let numbers = |els: &[rune::ElementInfo], e: &rune::ElementInfo| {
            let parent = &e.path[..e.path.rfind('/').unwrap()];
            at(els, &format!("{parent}/nn")).map(|n| n.target[0] + n.target[2]).expect("the new line numbers beside it")
        };
        let past = |els: &[rune::ElementInfo], e: &rune::ElementInfo| e.target[0] + e.target[2] - numbers(els, e);
        assert_eq!(past(&big, b), past(&plain, &a), "the comment button ends as far past the line numbers at any text size");
        pluses += 1;
    }
    assert!(pluses > 0, "comment buttons compared");
}
