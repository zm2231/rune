use rune_kit::shell::{TabItem, TabItemStatus, TabOrientation, TabStrip, Toolbar};

use super::*;

const SCALES: [f32; 3] = [1.0, 1.6, 2.5];

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
        let width = 760.0;
        let tabs = TabStrip::<G>::new(format!("tabs-scale-{scale}"), TabOrientation::Horizontal)
            .tabs([
                TabItem::new("notes", "Release notes draft").icon(&ic::PENCIL_LINE).closable(true),
                TabItem::new("build", "Build the release").status(Some(TabItemStatus::Busy)),
                TabItem::new("review", "Review comments"),
            ])
            .active(Some("notes"))
            .viewport_width(Some(width))
            .build(k);
        el().w(width).child(tabs)
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
