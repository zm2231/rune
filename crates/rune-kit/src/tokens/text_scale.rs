use rune::{KeyEvent, MenuItem, Provided, Shortcut, TextStyle, Tx};

use super::kit::{Kit, Type};
use super::metrics::{BySize, ButtonMetrics, IconButtonMetrics, Metrics};

/// The smallest and largest [`TextScale`].
pub const TEXT_SCALES: (f32, f32) = (0.5, 3.0);

const STEPS: [f32; 9] = [0.75, 0.85, 1.0, 1.15, 1.3, 1.5, 1.75, 2.0, 2.5];

fn clamp(scale: f32) -> f32 {
    if scale.is_finite() { scale.clamp(TEXT_SCALES.0, TEXT_SCALES.1) } else { 1.0 }
}

fn style(t: TextStyle, s: f32) -> TextStyle {
    TextStyle { size: t.size * s, line_height: t.line_height * s, tracking: t.tracking * s, ..t }
}

fn by_size<T: Copy>(b: BySize<T>, f: impl Fn(T) -> T) -> BySize<T> {
    BySize { mini: f(b.mini), small: f(b.small), regular: f(b.regular), large: f(b.large), extra_large: f(b.extra_large) }
}

impl Kit {
    /// This kit with its type at `scale` times the theme's own (clamped to [`TEXT_SCALES`]),
    /// whatever scale it had: every [`Type`] style, and the metrics that follow the type, where
    /// a control's height holds a line of text (buttons, fields, segmented controls, popup and
    /// disclosure rows, keybinds) or a glyph sits next to text (button and select icons, icon
    /// buttons, which grow by as much as their glyph). Padding, radii, gaps and layout stay.
    pub fn with_text_scale(&self, scale: f32) -> Kit {
        let to = clamp(scale);
        let s = to / self.text_scale;
        let mut k = self.clone();
        k.text_scale = to;
        if (s - 1.0).abs() < 1e-6 {
            return k;
        }
        let t = &self.ty;
        k.ty = Type {
            ui: style(t.ui, s),
            ui_medium: style(t.ui_medium, s),
            ui_compact: style(t.ui_compact, s),
            small: style(t.small, s),
            small_medium: style(t.small_medium, s),
            body: style(t.body, s),
            body_medium: style(t.body_medium, s),
            text_small: style(t.text_small, s),
            text_small_medium: style(t.text_small_medium, s),
            text_xsmall: style(t.text_xsmall, s),
            text_xsmall_medium: style(t.text_xsmall_medium, s),
            mono: style(t.mono, s),
        };
        k.metrics = self.text_metrics(s);
        k
    }

    fn text_metrics(&self, s: f32) -> Metrics {
        let mut m = self.metrics;
        m.button = Some(by_size(m.button.unwrap_or_else(|| ButtonMetrics::kit(self)), |b| ButtonMetrics { height: b.height * s, icon: b.icon * s, ..b }));
        m.icon_button = Some(by_size(m.icon_button.unwrap_or_else(|| IconButtonMetrics::kit(self)), |b| IconButtonMetrics {
            size: b.size + b.icon * (s - 1.0),
            icon: b.icon * s,
            ..b
        }));
        let f = self.field_metrics();
        m.field = Some(super::metrics::FieldMetrics { height: f.height * s, large_height: f.large_height * s, ..f });
        let a = self.text_area_metrics();
        m.text_area = Some(super::metrics::TextAreaMetrics { style: style(a.style, s), ..a });
        m.text_area_min_height = Some(self.text_area_min_height() * s);
        let g = self.segmented_metrics();
        m.segmented = Some(super::metrics::SegmentedMetrics { height: g.height * s, ..g });
        m.disclosure_header_height = Some(self.disclosure_header_height() * s);
        let kb = self.keybind_metrics();
        m.keybind = Some(super::metrics::KeybindMetrics { height: kb.height * s, ..kb });
        let d = self.dialog_metrics();
        m.dialog = Some(super::metrics::DialogMetrics { title: d.title.map(|t| style(t, s)), ..d });
        let r = self.popup_row_metrics();
        m.popup_row = Some(super::metrics::PopupRowMetrics { height: r.height * s, ..r });
        let sel = self.select_metrics();
        m.select = Some(super::metrics::SelectMetrics { chevron: sel.chevron * s, chevron_box: sel.chevron_box * s });
        m
    }

    /// An app's own `style` at this kit's [`Kit::text_scale`].
    pub fn text(&self, style: TextStyle) -> TextStyle {
        self::style(style, self.text_scale)
    }

    /// An app's own size that follows its text (a row that holds a line, an icon beside a
    /// label), at this kit's [`Kit::text_scale`].
    pub fn text_size(&self, points: f32) -> f32 {
        points * self.text_scale
    }
}

/// A step of the standard text size commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextStep {
    Bigger,
    Smaller,
    Normal,
}

impl TextStep {
    /// The step Cmd (Ctrl off macOS) with `=` or `+`, `-`, or `0` asks for, as browsers and
    /// macOS apps take them; None for any other key. For a window without a native menu bar;
    /// with one, [`TextScale::menu_items`] takes the keys.
    pub fn from_key(ev: &KeyEvent) -> Option<TextStep> {
        if ev.is_command('=') || ev.is_command('+') {
            Some(TextStep::Bigger)
        } else if ev.is_command('-') {
            Some(TextStep::Smaller)
        } else if ev.is_command('0') {
            Some(TextStep::Normal)
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub(crate) struct Scale(pub(crate) f32);

/// A text size an app keeps in its state and gives a part of its view with
/// [`KitCx::with_text_scale`](crate::KitCx::with_text_scale): kits read there
/// ([`KitCx::kit`](crate::KitCx::kit)) have their type at this scale ([`Kit::with_text_scale`]),
/// and the rest of the layout stays as it is. Memos built under it build again when the app
/// replaces it with another.
#[derive(Clone, Debug)]
pub struct TextScale(pub(crate) Provided<Scale>);

impl Default for TextScale {
    fn default() -> Self {
        Self::new(1.0)
    }
}

impl TextScale {
    /// `scale` times the theme's type, clamped to [`TEXT_SCALES`].
    pub fn new(scale: f32) -> Self {
        Self(Provided::new(Scale(clamp(scale))))
    }

    pub fn get(&self) -> f32 {
        self.0.0
    }

    /// The scale a step away: the next larger or smaller of 75, 85, 100, 115, 130, 150, 175,
    /// 200 and 250 percent (from a scale between two, the nearer of them in that direction),
    /// or 100 percent for [`TextStep::Normal`]. The same scale when there is no step further.
    pub fn step(&self, step: TextStep) -> TextScale {
        let now = self.get();
        let next = match step {
            TextStep::Normal => 1.0,
            TextStep::Bigger => STEPS.iter().copied().find(|&v| v > now + 1e-3).unwrap_or(now),
            TextStep::Smaller => STEPS.iter().rev().copied().find(|&v| v < now - 1e-3).unwrap_or(now),
        };
        if (next - now).abs() < 1e-6 { self.clone() } else { TextScale::new(next) }
    }

    /// The View menu's Make Text Bigger (Cmd+=), Make Text Smaller (Cmd+-) and Make Text Normal
    /// Size (Cmd+0), stepping the scale `lens` finds in the app's state (Ctrl off macOS). Add
    /// them with `App::menu_item`; [`TextStep::from_key`] takes the same keys where there is no
    /// native menu.
    pub fn menu_items<S: 'static>(lens: fn(&mut S) -> &mut TextScale) -> [MenuItem<S>; 3] {
        let item = |title: &str, id: &str, key: &str, step: TextStep| {
            let shortcut: Shortcut = key.parse().unwrap_or_else(|e| panic!("{key}: {e:?}"));
            MenuItem::with_action(title, move |s: &mut S, _: &mut Tx| {
                let scale = lens(s);
                *scale = scale.step(step);
            })
            .id(id)
            .menu("View")
            .key_equivalent(shortcut)
        };
        [
            item("Make Text Bigger", "text-bigger", "mod+equal", TextStep::Bigger),
            item("Make Text Smaller", "text-smaller", "mod+minus", TextStep::Smaller),
            item("Make Text Normal Size", "text-normal", "mod+0", TextStep::Normal),
        ]
    }
}

#[cfg(test)]
#[path = "text_scale_tests.rs"]
mod tests;
