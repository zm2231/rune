use super::*;

const SERVE: &str = "opencode serve --port 4096 --hostname 0.0.0.0";

/// A settings row: a command people can select and copy, highlighted as the kit's fields
/// highlight a selection, and an icon-only copy button that explains itself on hover.
pub(super) fn text_selectable_help(s: &G, k: &Kit, _: &mut Vec<El>) -> El {
    let c = &k.c;
    col().w(480.0).gap(8.0)
        .child(caption(k, "Run the server from a terminal"))
        .child(
            row().key("command-row").h(36.0).pl(12.0).pr(4.0).gap(8.0).center_y()
                .fill(c.surface_base).radius(k.radius.md).border(1.0, c.border_weak)
                .child(text(SERVE).key("command").font(k.ty.mono).color(c.text_base).grow(1.0).shrink()
                    .selectable(true).selection_color(k.selection()))
                .child(IconButton::new(&ic::COPY).variant(IconButtonVariant::Ghost).control_size(ControlSize::Small)
                    .label("Copy command").on_press_tx(|_: &mut G, tx: &mut Tx| tx.copy(SERVE)).build(k)
                    .key("copy").help("Copy command")),
        )
        .child(row().w(280.0).child(TextInput::new(&s.edits[7]).placeholder("Selected like a field")
            .on_edit(|s: &mut G| &mut s.edits[7]).build(k).key("field")))
}

const LONG_PATH: &str = "~/Projects/rune/crates/rune-kit/src/components/settings/appearance_panel.rs";

/// A path keeps its folder and file name as its box narrows, beside the end and head cuts, each
/// with a `Type::caption` line under it.
pub(super) fn text_truncate_middle(_: &G, k: &Kit, _: &mut Vec<El>) -> El {
    let c = &k.c;
    let path = |w: f32, cut: fn(El) -> El, how: &str| col().w(w)
        .child(cut(text(LONG_PATH).font(k.ty.ui).color(c.text_base)).selectable(true).selection_color(k.selection()))
        .child(text(format!("{how}, {w} pt")).font(k.ty.caption()).color(c.text_weak));
    col().gap(12.0)
        .child(path(360.0, El::truncate_middle, "Middle"))
        .child(path(240.0, El::truncate_middle, "Middle"))
        .child(path(160.0, El::truncate_middle, "Middle"))
        .child(path(240.0, El::truncate, "End"))
        .child(path(240.0, El::truncate_head, "Head"))
}

const LONG_FILENAME: &str = "Quarterly report final revised version two of the budget.xlsx";

/// Two-line filename cards comparing `max_lines(2).elide_middle()`, `max_lines(2).elide_head()`,
/// and default end elision: middle elision keeps both the opening words and the `.xlsx` suffix.
pub(super) fn text_max_lines_elide(_: &G, k: &Kit, _: &mut Vec<El>) -> El {
    let c = &k.c;
    let card = |title: &str, label: &str, w: f32, build: fn(El) -> El| {
        col()
            .w(w)
            .padding(10.0)
            .gap(6.0)
            .radius(k.radius.md)
            .fill(c.surface_base)
            .border(1.0, c.border_weak)
            .child(text(title).font(k.ty.caption()).color(c.text_weak))
            .child(
                build(text(label).font(k.ty.ui).color(c.text_base))
                    .selectable(true)
                    .selection_color(k.selection()),
            )
    };
    row()
        .gap(12.0)
        .flex_wrap()
        .child(card("2 lines · elide_middle", LONG_FILENAME, 160.0, |e| e.max_lines(2).elide_middle()))
        .child(card("2 lines · elide_head", LONG_PATH, 180.0, |e| e.wrap_anywhere().max_lines(2).elide_head()))
        .child(card("2 lines · default (end)", LONG_FILENAME, 160.0, |e| e.max_lines(2)))
}

pub(crate) fn focus_caret_sample() -> String {
    (1..=30)
        .map(|i| format!("Line {i:02}: multi-line entry with caret parked at the end."))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) const NOWRAP_CODE: &str = concat!(
    "pub fn render_file_entry(name: &str, max_width: f32, elide_middle: bool) -> El {\n",
    "    let label = text(name).w(max_width).max_lines(2);\n",
    "    if elide_middle { label.elide_middle() } else { label }\n",
    "}\n",
    "// Long unwrapped line that extends past the viewport width so scroll_xy scrolls sideways under the caret.\n",
    "fn main() { println!(\"hello from an unwrapped multiline text_input\"); }",
);

/// An unwrapped multi-line code `text_input` (`El::nowrap`) inside a `scroll_xy` viewport:
/// long lines keep their natural width and scroll sideways as the caret moves.
pub(super) fn text_nowrap_input(s: &G, k: &Kit, _: &mut Vec<El>) -> El {
    let c = &k.c;
    let border = if s.nowrap_code.focused { c.border_strong } else { c.border_weak };
    col()
        .gap(6.0)
        .child(caption(k, "Unwrapped multi-line text_input (.nowrap() inside .scroll_xy())"))
        .child(
            col()
                .key("nowrap-viewport")
                .w(340.0)
                .h(140.0)
                .radius(k.radius.md)
                .fill(c.surface_base)
                .border(1.0, border)
                .scroll_xy()
                .child(
                    rune::text_input(&s.nowrap_code, k.ty.mono)
                        .key("nowrap-code")
                        .label("Code editor")
                        .nowrap()
                        .padding(10.0)
                        .color(c.text_base)
                        .caret(c.text_base, k.selection())
                        .on_edit(|s: &mut G| &mut s.nowrap_code)
                        .on_submit(|s: &mut G, _| {
                            let _ = s.nowrap_code.insert("\n");
                        }),
                )
                .scroll_bar_styled(ScrollBarStyle::from_kit(k)),
        )
}

/// Tall 48 pt single-line `text_input` fields showing opt-in `El::align_y`: vertically centred
/// text + caret (`Align::Center`), centred placeholder, bottom-aligned (`Align::End`), and the
/// default top-aligned layout (`Align::Start`).
pub(super) fn text_align_y_input(s: &G, k: &Kit, _: &mut Vec<El>) -> El {
    let c = &k.c;
    let st = TextStyle::new(14.0, 400.0).lh(20.0);
    let field = |key: &'static str, label: &'static str, edit: &TextEdit, placeholder: &'static str, align: rune::Align, idx: usize| {
        let border = if edit.focused { c.border_strong } else { c.border_weak };
        col()
            .gap(4.0)
            .child(caption(k, label))
            .child(
                rune::text_input(edit, st)
                    .single_line()
                    .key(key)
                    .label(label)
                    .w(340.0)
                    .h(48.0)
                    .px(12.0)
                    .align_y(align)
                    .radius(k.radius.md)
                    .fill(c.surface_base)
                    .border(1.0, border)
                    .color(c.text_base)
                    .placeholder(placeholder, c.text_faint)
                    .caret(c.text_base, k.selection())
                    .on_edit(rune::edit_access(move |s: &mut G| &mut s.align_y_edits[idx])),
            )
    };
    col()
        .gap(12.0)
        .child(field("align-y-center", "48 pt single_line · align_y(Align::Center) (text + caret)", &s.align_y_edits[0], "Type here…", rune::Align::Center, 0))
        .child(field("align-y-placeholder", "48 pt single_line · align_y(Align::Center) (placeholder)", &s.align_y_edits[1], "Search notes…", rune::Align::Center, 1))
        .child(field("align-y-end", "48 pt single_line · align_y(Align::End)", &s.align_y_edits[2], "Bottom-aligned…", rune::Align::End, 2))
        .child(field("align-y-start", "48 pt single_line · default (Align::Start)", &s.align_y_edits[3], "Top-aligned…", rune::Align::Start, 3))
}
