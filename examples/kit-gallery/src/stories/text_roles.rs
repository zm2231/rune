use rune::Color;
use rune_kit::KitEnv;

use super::*;

fn line(k: &Kit, role: &str, parts: Vec<El>) -> El {
    row()
        .gap(12.0)
        .center_y()
        .child(el().w(k.text_size(116.0)).child(text(role.to_owned()).font(k.ty.text_xsmall).color(k.c.text_weak)))
        .child(row().gap(6.0).center_y().children(parts))
}

fn words(k: &Kit, s: &str, color: Color) -> El {
    text(s.to_owned()).font(k.ty.text_small).color(color)
}

fn panel(k: &Kit) -> El {
    let r = &k.roles;
    let code = |s: &str| text(s.to_owned()).font(k.ty.mono).color(r.code);
    let title = match (k.dark, k.increased_contrast) {
        (false, false) => "Light",
        (true, false) => "Dark",
        (false, true) => "Light, increased contrast",
        (true, true) => "Dark, increased contrast",
    };
    col()
        .key(format!("text-roles-{}", if k.dark { "dark" } else { "light" }))
        .gap(10.0)
        .padding(16.0)
        .fill(k.c.bg_base)
        .border(1.0, k.c.border_weak)
        .radius(k.radius.md)
        .child(text(title).font(k.ty.text_small_medium).color(k.c.text_strong))
        .child(line(k, "working", vec![words(k, "● Live · Recording 02:14", r.working)]))
        .child(line(k, "meta", vec![words(k, "Started 12:02 · 3 listeners", r.meta)]))
        .child(line(k, "peer", vec![words(k, "Sam: the build is green on my side", r.peer)]))
        .child(line(k, "link", vec![words(k, "Release notes", r.link)]))
        .child(line(k, "code", vec![code("cargo test -p rune-kit")]))
        .child(line(k, "added", vec![words(k, "+42", r.added), code("src/tokens/roles.rs")]))
        .child(line(k, "removed", vec![words(k, "−7", r.removed), code("src/tokens/legacy.rs")]))
        .child(line(k, "needs_attention", vec![words(k, "Waiting on your review", r.needs_attention)]))
        .child(line(k, "done", vec![words(k, "Tests passed", r.done)]))
        .child(line(k, "error", vec![words(k, "Deploy failed: exit 1", r.error)]))
}

pub(super) fn roles(s: &G, k: &Kit, _: &mut Vec<El>) -> El {
    let increased = k.increased_contrast || s.is("roles-increased");
    let kit = |dark: bool| s.theme.kit(&KitEnv::new(dark).increased_contrast(increased));
    col()
        .gap(16.0)
        .items_start()
        .child(Switch::new(increased).label("Increase contrast").on_toggle(flip("roles-increased")).build(k))
        .child(flex(16.0).items_start().child(panel(&kit(false))).child(panel(&kit(true))))
}
