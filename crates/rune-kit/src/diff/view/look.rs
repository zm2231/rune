//! Diff palette, colours, typography, and gutter metrics derived from [`Kit`].

use std::hash::{Hash, Hasher};

use rune::{Color, Icon, TextStyle};

use super::DiffView;
use crate::Kit;
use crate::diff::model::LineKind;
use crate::diff::state::RowHeights;
use crate::syntax::SyntaxTheme;
use crate::tokens::palette::tint;

/// Colours and type, from the kit's palette so themes restyle them.
pub(super) struct Look {
    pub(super) font: TextStyle,
    pub(super) rows: RowHeights,
    pub(super) icon: f32,
    pub(super) label: TextStyle,
    pub(super) small: TextStyle,
    /// [`FoldRow::LineInfo`]'s count: the interface face at the code's size, on the row's height.
    pub(super) separator_text: TextStyle,
    /// [`FoldRow::LineInfo`]'s row: the background 2% toward black, 5% toward white in dark
    /// (OpenCode's `--diffs-bg-separator-override`).
    pub(super) separator: Color,
    /// The advance of a mono column.
    pub(super) adv: f32,
    pub(super) bg: Color,
    pub(super) text: Color,
    pub(super) muted: Color,
    pub(super) faint: Color,
    pub(super) border: Color,
    pub(super) header_bg: Color,
    pub(super) hover: Color,
    pub(super) add: Tints,
    pub(super) del: Tints,
    pub(super) gap_bg: Color,
    pub(super) empty_bg: Color,
    pub(super) selected: Color,
    pub(super) selected_gutter: Color,
    pub(super) selection: Color,
    pub(super) accent: Color,
    pub(super) on_accent: Color,
    pub(super) triangle_down: &'static Icon,
    pub(super) triangle_right: &'static Icon,
    pub(super) syntax: SyntaxTheme,
    pub(super) floor: f32,
    /// Changes when anything above does, for memoised rows.
    pub(super) version: u64,
}

/// What an app overrides in a [`Look`] through [`DiffView`]'s builder.
#[derive(Default)]
pub(super) struct LookSpec {
    syntax_theme: Option<SyntaxTheme>,
    code_font: Option<TextStyle>,
}

/// A line height of 0 means 1.25 × size in [`TextStyle`]; rows need the number.
fn resolve_line_height(font: TextStyle) -> TextStyle {
    let line_height = if font.line_height > 0.0 { font.line_height } else { (font.size * 1.25).ceil() };
    TextStyle { line_height, ..font }
}

impl<S: 'static> DiffView<S> {
    /// Override the syntax highlighting palette (by default [`SyntaxTheme::from_kit`]). On an
    /// added or removed line, a word-change highlight or a selected row, a colour under 4.5:1
    /// contrast on what it sits on (7:1 with the kit's
    /// [`increased_contrast`](crate::Kit::increased_contrast)) draws moved toward black or white
    /// along its hue until it reaches it; unchanged lines draw the theme's colours as given.
    ///
    /// ```
    /// # use rune_kit::{Kit, diff::{DiffState, DiffView}};
    /// # struct App { diff: DiffState }
    /// # let s = &App { diff: DiffState::default() };
    /// # let k = Kit::neutral(true);
    /// use rune::Color;
    /// use rune_kit::syntax::{Syntax, SyntaxTheme};
    ///
    /// let theme = SyntaxTheme::from_kit(&k).with(Syntax::Keyword, Color { r: 0.9, g: 0.3, b: 0.6, a: 1.0 });
    /// # let _ =
    /// DiffView::new(|s: &App| &s.diff, |s: &mut App| &mut s.diff)
    ///     .syntax_theme(theme)
    ///     .build(s, &k)
    /// # ;
    /// ```
    pub fn syntax_theme(mut self, theme: SyntaxTheme) -> Self {
        self.look.syntax_theme = Some(theme);
        self
    }

    /// The face code and line numbers draw in (by default the kit's mono at 13 pt on 20 pt
    /// lines). Each code line is the font's line height tall (1.25 × size when it is 0), and
    /// a column is 0.6 × size wide, so pick a monospaced family.
    ///
    /// ```
    /// # use rune_kit::{Kit, diff::{DiffState, DiffView}};
    /// # struct App { diff: DiffState }
    /// # let s = &App { diff: DiffState::default() };
    /// # let k = Kit::neutral(true);
    /// use rune::TextStyle;
    ///
    /// # let _ =
    /// DiffView::new(|s: &App| &s.diff, |s: &mut App| &mut s.diff)
    ///     .code_font(TextStyle { size: 12.0, line_height: 20.0, ..k.ty.mono })
    ///     .build(s, &k)
    /// # ;
    /// ```
    pub fn code_font(mut self, font: TextStyle) -> Self {
        self.look.code_font = Some(font);
        self
    }
}

#[derive(Clone, Copy)]
pub(super) struct Tints {
    pub(super) line: Color,
    pub(super) gutter: Color,
    pub(super) word: Color,
    pub(super) number: Color,
    pub(super) bar: Color,
    pub(super) count: Color,
}

impl Look {
    pub(super) fn new(k: &Kit, spec: LookSpec) -> Look {
        let c = &k.c;
        let bg = c.bg_base;
        let dark = k.dark;
        let tints = |hue: Color, fg: Color| Tints {
            line: k.change_tint(bg, hue, if dark { 0.13 } else { 0.1 }),
            gutter: k.change_tint(bg, hue, if dark { 0.2 } else { 0.17 }),
            word: k.change_tint(bg, hue, if dark { 0.32 } else { 0.27 }),
            number: tint(c.text_muted, hue, 0.7),
            bar: hue,
            count: fg,
        };
        let font = spec.code_font.map_or(TextStyle { size: k.text_size(13.0), line_height: k.text_size(RowHeights::BASE.line), ..k.ty.mono }, resolve_line_height);
        let info = c.info.icon;
        let base = RowHeights::BASE;
        let rows = RowHeights { line: font.line_height, header: k.text_size(base.header), gap: k.text_size(base.gap), note: k.text_size(base.note) };
        let mut look = Look {
            font,
            rows,
            icon: k.text_size(16.0),
            label: TextStyle { size: k.text_size(13.0), line_height: k.text_size(20.0), ..k.ty.ui_medium },
            small: TextStyle { size: k.text_size(12.0), line_height: k.text_size(16.0), ..k.ty.mono },
            separator_text: TextStyle { size: font.size, line_height: rows.gap, ..k.ty.ui },
            separator: tint(bg, if dark { c.white } else { Color::BLACK }, if dark { 0.05 } else { 0.02 }),
            adv: 0.6 * font.size,
            bg,
            text: c.text_base,
            muted: c.text_muted,
            faint: c.text_faint,
            border: c.border_weak,
            header_bg: c.layer_01,
            hover: k.alpha(c.text_base, if dark { 0.06 } else { 0.035 }),
            add: tints(k.roles.added, c.success.fg),
            del: tints(k.roles.removed, c.danger.fg),
            gap_bg: tint(bg, info, if dark { 0.1 } else { 0.06 }),
            empty_bg: tint(bg, c.text_base, if dark { 0.03 } else { 0.025 }),
            selected: tint(bg, info, if dark { 0.2 } else { 0.14 }),
            selected_gutter: tint(bg, info, if dark { 0.32 } else { 0.26 }),
            selection: k.selection(),
            accent: info,
            on_accent: c.white,
            triangle_down: k.glyph.triangle_down,
            triangle_right: k.glyph.triangle_right,
            syntax: spec.syntax_theme.unwrap_or_else(|| SyntaxTheme::from_kit(k)),
            floor: k.text_floor(),
            version: 0,
        };
        let mut h = std::hash::DefaultHasher::new();
        for c in [look.bg, look.text, look.add.bar, look.add.line, look.del.bar, look.del.line, look.accent, look.header_bg] {
            [c.r, c.g, c.b, c.a].map(f32::to_bits).hash(&mut h);
        }
        look.font.hash(&mut h);
        look.floor.to_bits().hash(&mut h);
        [rows.header, rows.gap, rows.note].map(f32::to_bits).hash(&mut h);
        look.syntax.hash(&mut h);
        look.version = h.finish();
        look
    }

    pub(super) fn tints(&self, kind: LineKind) -> Option<&Tints> {
        match kind {
            LineKind::Added => Some(&self.add),
            LineKind::Removed => Some(&self.del),
            LineKind::Context => None,
        }
    }

    pub(super) fn gutter_fill(&self, kind: LineKind, selected: bool) -> Color {
        match (selected, self.tints(kind)) {
            (true, _) => self.selected_gutter,
            (false, Some(t)) => t.gutter,
            (false, None) => self.bg,
        }
    }

    /// The height of a code line: the code font's line height.
    pub(super) fn line_h(&self) -> f32 { self.rows.line }

    /// Where the comment button sits in a line's gutter: centred on the first line.
    pub(super) fn plus_y(&self) -> f32 { ((self.line_h() - self.icon) / 2.0).max(0.0) }

    pub(super) fn plus_x(&self, numbers_end: f32) -> f32 { numbers_end + 6.0 - self.icon }

    /// One line-number column for numbers of `digits` digits.
    pub(super) fn number_w(&self, digits: u32) -> f32 {
        (digits as f32 * self.adv + 16.0).ceil()
    }
}
