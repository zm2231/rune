use rune_kit::shell::{RowStatus, Sidebar, SidebarRow, TabItem, TabItemStatus, TabOrientation, TabStrip};

use super::*;

fn avatar(k: &Kit, initial: &str, fill: rune::Color) -> El {
    let side = k.text_size(16.0);
    row()
        .size(side, side)
        .center()
        .radius(4.0)
        .fill(fill)
        .child(text(initial.to_owned()).font(k.text(TextStyle::new(10.0, 600.0).lh(16.0))).color(k.c.text_base))
}

fn framed(k: &Kit, key: &str, [width, height]: [f32; 2], child: El) -> El {
    col()
        .key(key.to_owned())
        .size(k.text_size(width), k.text_size(height) + 12.0)
        .py(6.0)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_weak)
        .radius(k.radius.md)
        .child(child)
}

pub(super) fn tab_status(_: &G, k: &Kit, _: &mut Vec<El>) -> El {
    let tabs = || {
        [
            TabItem::new("draft", "Draft").icon(&ic::PENCIL_LINE).closable(true),
            TabItem::new("build", "Build the release").status(Some(TabItemStatus::Busy)).closable(true),
            TabItem::new("review", "Review").status(Some(TabItemStatus::Attention)),
            TabItem::new("inbox", "Inbox").status(Some(TabItemStatus::Unread)),
            TabItem::new("deploy", "Deploy").status(Some(TabItemStatus::Failed)),
        ]
    };
    let horizontal = TabStrip::<G>::new("tabs-status", TabOrientation::Horizontal)
        .tabs(tabs())
        .active(Some("build"))
        .viewport_width(Some(k.text_size(640.0)))
        .build(k);
    let vertical = TabStrip::<G>::new("tabs-status-vertical", TabOrientation::Vertical)
        .tabs([
            TabItem::new("rune", "Text scale").subtitle("rune").avatar(avatar(k, "R", k.c.layer_03)).status(Some(TabItemStatus::Busy)),
            TabItem::new("site", "Docs pages").subtitle("site").avatar(avatar(k, "S", k.c.layer_02)).status(Some(TabItemStatus::Unread)),
        ])
        .active(Some("rune"))
        .build(k);
    col()
        .gap(16.0)
        .items_start()
        .child(caption(k, "Horizontal: a busy tab beside attention, unread and failed marks"))
        .child(el().w(k.text_size(640.0)).child(horizontal))
        .child(caption(k, "Vertical, two lines with avatars"))
        .child(framed(k, "tabs-status-vertical-frame", [240.0, 100.0], vertical))
}

pub(super) fn sidebar_two_line(_: &G, k: &Kit, _: &mut Vec<El>) -> El {
    let rows = Sidebar::new()
        .label("Sessions")
        .inset(6.0)
        .child(SidebarRow::new("Text scale").icon(&ic::FOLDER).subtitle("rune \u{b7} 3 files changed").status(RowStatus::Busy).selected(true).build(k))
        .child(SidebarRow::new("Docs pages").avatar(avatar(k, "S", k.c.layer_03)).subtitle("site \u{b7} waiting on review").status(RowStatus::Attention).build(k))
        .child(SidebarRow::new("Release notes").avatar(avatar(k, "N", k.c.layer_02)).subtitle("2 new replies").status(RowStatus::Unread).build(k))
        .child(SidebarRow::new("Inbox").icon(&ic::FOLDER).badge("12").build(k));
    framed(k, "sidebar-two-line", [260.0, 160.0], rows.build(k))
}

pub(super) fn avatar_rows(s: &G, k: &Kit, layer: &mut Vec<El>) -> El {
    let open = is_open(s, "avatar-menu");
    let rows = Sidebar::new()
        .label("People")
        .inset(6.0)
        .child(SidebarRow::new("Ada").avatar(avatar(k, "A", k.c.layer_03)).build(k))
        .child(SidebarRow::new("Grace").avatar(avatar(k, "G", k.c.layer_02)).badge("Owner").build(k))
        .child(SidebarRow::new("Linus").avatar(avatar(k, "L", k.c.layer_03)).status(RowStatus::Unread).build(k));
    let tabs = TabStrip::<G>::new("avatar-tabs", TabOrientation::Horizontal)
        .tabs([
            TabItem::new("ada", "Ada").avatar(avatar(k, "A", k.c.layer_03)),
            TabItem::new("grace", "Grace").avatar(avatar(k, "G", k.c.layer_02)).status(Some(TabItemStatus::Busy)),
        ])
        .active(Some("ada"))
        .viewport_width(Some(k.text_size(360.0)))
        .build(k);
    let menu = Menu::new("avatar-menu")
        .trigger(Button::new("Share with").expanded(open).on_press(toggle("avatar-menu")).build(k))
        .item(MenuItem::new("Ada Lovelace").lead(avatar(k, "A", k.c.layer_03)).on_select(close))
        .item(MenuItem::new("Grace Hopper").lead(avatar(k, "G", k.c.layer_02)).checked(true).on_select(close))
        .open(open)
        .on_close(close)
        .build(k, layer);
    col()
        .gap(16.0)
        .items_start()
        .child(framed(k, "avatar-sidebar", [220.0, 96.0], rows.build(k)))
        .child(el().w(k.text_size(360.0)).child(tabs))
        .child(row().child(menu))
}
