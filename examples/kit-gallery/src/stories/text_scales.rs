use rune_kit::shell::{TabItem, TabItemStatus, TabOrientation, TabStrip, Toolbar};

use super::*;

const SCALES: [f32; 3] = [1.0, 1.6, 2.5];

const OVERFLOW_WIDTH: f32 = 420.0;

fn at_scales(k: &Kit, mut f: impl FnMut(&Kit, f32) -> El) -> El {
    let mut c = col().gap(16.0).items_start();
    for scale in SCALES {
        c = c.child(caption(k, &format!("Text at {scale}x"))).child(f(&k.with_text_scale(scale), scale));
    }
    c
}

pub(super) fn toolbar(_: &G, k: &Kit, _: &mut Vec<El>) -> El {
    at_scales(k, |k, scale| {
        let mut bar = Toolbar::new().leading(text("Quarterly planning notes for the design review").font(k.ty.ui_medium).color(k.c.text_base).truncate().shrink().min_w(0.0));
        for (glyph, label) in [(&ic::MAGNIFYING_GLASS, "Search"), (&ic::PLUS, "New"), (&ic::PENCIL_LINE, "Edit"), (&ic::ARCHIVE, "Archive"), (&ic::FORK, "Fork")] {
            bar = bar.trailing(IconButton::new(glyph).label(label).build(k));
        }
        el().key(format!("toolbar-{scale}")).w(420.0).fill(k.c.layer_01).border(1.0, k.c.border_weak).radius(k.radius.md).child(bar.build(k))
    })
}

pub(super) fn tab_strip(_: &G, k: &Kit, _: &mut Vec<El>) -> El {
    at_scales(k, |k, scale| {
        let width = k.text_size(460.0);
        let tabs = TabStrip::<G>::new(format!("tabs-scale-{scale}"), TabOrientation::Horizontal)
            .tabs([
                TabItem::new("notes", "Release notes draft").icon(&ic::PENCIL_LINE).closable(true),
                TabItem::new("build", "Build the release").status(Some(TabItemStatus::Busy)),
            ])
            .active(Some("notes"))
            .viewport_width(Some(width))
            .build(k);
        el().w(width).child(tabs)
    })
}

pub(super) fn tab_strip_overflow(_: &G, k: &Kit, _: &mut Vec<El>) -> El {
    at_scales(k, |k, scale| {
        let titles = ["Release notes draft", "Build the release", "Design review", "Quarterly plan", "Bug triage", "Onboarding guide", "Billing", "Roadmap"];
        let tabs = TabStrip::<G>::new(format!("tabs-overflow-{scale}"), TabOrientation::Horizontal)
            .tabs(titles.iter().enumerate().map(|(i, t)| TabItem::new(format!("t{i}"), *t).closable(true)))
            .active(Some("t0"))
            .viewport_width(Some(OVERFLOW_WIDTH))
            .build(k);
        el().w(OVERFLOW_WIDTH).child(tabs)
    })
}

pub(super) fn segmented_tabs(_: &G, k: &Kit, _: &mut Vec<El>) -> El {
    at_scales(k, |k, _| {
        let sections = ["General", "Appearance", "Notifications", "Privacy", "Accounts", "Advanced"];
        col().w(OVERFLOW_WIDTH).child(SegmentedControl::new(&sections, Some(0)).label("Settings sections").as_tabs().fit().build(k))
    })
}

pub(super) fn scroll_overflow_chips(_: &G, k: &Kit, _: &mut Vec<El>) -> El {
    at_scales(k, |k, scale| {
        let chips = ["All", "Unread", "Mentions", "Drafts", "Starred", "Shared with me", "Archived", "Snoozed", "Sent"]
            .map(|label| Badge::new(label).build(k));
        let viewport = row().scroll_x().scroll_edge_fade(16.0).child(row().gap(6.0).py(4.0).children(chips));
        el().w(OVERFLOW_WIDTH).fill(k.c.bg_base).child(ScrollOverflow::new(format!("chips-{scale}"), viewport).build(k).w_full())
    })
}

pub(super) fn textarea_placeholder(_: &G, k: &Kit, _: &mut Vec<El>) -> El {
    let empty = TextEdit::default();
    at_scales(k, |k, _| {
        col()
            .gap(8.0)
            .items_start()
            .child(TextArea::new(&empty).placeholder("Describe what changed, why it changed, and how a reviewer can check it").width(Some(240.0)).build(k))
            .child(Button::new("Send").build(k))
    })
}
