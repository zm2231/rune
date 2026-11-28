//! A window's toolbar: leading, centre and trailing slots in a band the window drags by.

use rune::{El, SystemMaterial, row, text};

use crate::Kit;

/// The toolbar band's height at text scale 1, macOS 26's unified toolbar. The traffic lights
/// centre in it when the window places them at y = (48 - 14) / 2 = 17. A larger kit text scale
/// grows the band ([`Toolbar::height`]).
pub const TOOLBAR_HEIGHT: f32 = 48.0;
/// Padding at the band's ends.
const PAD: f32 = 12.0;
const GAP: f32 = 8.0;
const ITEM_INSET: f32 = 8.0;
const LEADING_MIN: f32 = 96.0;
const TRAILING_FADE: f32 = 16.0;
const RING_ROOM: f32 = 4.0;
const TRAILING_SHRINK: f32 = 1e-4;

/// A toolbar: `Toolbar::new().leading(..).title("..").trailing(..).build(&kit)`. The band is a
/// window drag region (buttons in it keep their presses) and its centre slot sits in the
/// middle of the band whatever the sides hold.
///
/// When the sides don't fit, a long leading title truncates down to 96 points (at the kit's
/// [`Kit::text_scale`]) and keeps that much; past it the trailing slot gives way: items in it
/// that shrink do, and the rest fade out toward its leading edge, clear of the title.
pub struct Toolbar {
    leading: Vec<El>,
    center: Option<El>,
    title: Option<String>,
    trailing: Vec<El>,
    pad: Option<f32>,
    material: Option<SystemMaterial>,
}

impl Toolbar {
    pub fn new() -> Toolbar {
        Toolbar { leading: Vec::new(), center: None, title: None, trailing: Vec::new(), pad: None, material: None }
    }
    pub fn leading(mut self, e: El) -> Self {
        self.leading.push(e);
        self
    }
    pub fn trailing(mut self, e: El) -> Self {
        self.trailing.push(e);
        self
    }
    /// The centre slot. Replaces a [`Toolbar::title`].
    pub fn center(mut self, e: El) -> Self {
        self.center = Some(e);
        self.title = None;
        self
    }
    /// A centred title in the kit's medium UI type.
    pub fn title(mut self, t: impl Into<String>) -> Self {
        self.title = Some(t.into());
        self.center = None;
        self
    }
    /// Backs the band with a system material (macOS blurs what's behind a transparent
    /// window). Elsewhere, and headless, the kit's base background draws instead.
    pub fn material(mut self, m: SystemMaterial) -> Self {
        self.material = Some(m);
        self
    }

    /// The leading padding a `SplitView` works out (traffic lights and its sidebar toggle).
    pub(super) fn padding(mut self, points: f32) -> Self {
        self.pad = Some(points);
        self
    }

    /// The band's height in `k`: [`TOOLBAR_HEIGHT`], or more when the kit's text scale makes a
    /// regular button or a large icon button too tall to sit in it 8 points clear of each edge.
    pub fn height(k: &Kit) -> f32 {
        let control = k.button_metrics(crate::ControlSize::Regular).height.max(k.icon_button_metrics(crate::ControlSize::Large).size);
        TOOLBAR_HEIGHT.max(control + 2.0 * ITEM_INSET)
    }

    pub fn build(self, k: &Kit) -> El {
        let c = &k.c;
        let pl = self.pad.unwrap_or(PAD);
        let mut band =
            row().key("toolbar").role("toolbar").w_full().h(Self::height(k)).pl(pl).pr(PAD).gap(GAP).center_y().window_drag();
        if let Some(t) = &self.title {
            band = band.label(t.clone());
        }
        let center = match (self.center, self.title) {
            (Some(e), _) => Some(e),
            (None, Some(t)) => Some(text(t).key("title").font(k.ty.ui_medium).color(c.text_base).truncate()),
            _ => None,
        };
        if let Some(center) = center {
            // Across the whole band and centred in it, so the sides can differ in width. First,
            // so the side slots draw and take presses above it.
            band = band.child(
                row().key("center").pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).center().px(96.0).child(center),
            );
        }
        let leading_min = if self.leading.is_empty() { 0.0 } else { k.text_size(LEADING_MIN) };
        let fade = k.text_size(TRAILING_FADE).max(2.0 * GAP);
        band = band
            .child(row().key("leading").gap(4.0).center_y().shrink().min_w(leading_min).children(self.leading))
            .child(row().key("spacer").grow(1.0).a11y_hidden())
            .child(
                row()
                    .key("trailing")
                    .gap(4.0)
                    .center_y()
                    .justify_end()
                    .self_stretch()
                    .shrink_by(TRAILING_SHRINK)
                    .min_w(0.0)
                    .pl(fade)
                    .ml(-2.0 * GAP)
                    .pr(RING_ROOM)
                    .mr(-RING_ROOM)
                    .clip_fade_linear(fade, 0.0, 0.0, 0.0)
                    .children(self.trailing),
            );
        if let Some(m) = self.material {
            band = band.system_material(m).fill(c.bg_base);
        }
        band
    }
}

impl Default for Toolbar {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "toolbar_tests.rs"]
mod tests;
