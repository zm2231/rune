use rune::Color;

use super::kit::Kit;
use super::palette::{contrast, tint};
use super::roles::{TextRole, TextRoles};

impl Kit {
    /// The kit for a system that asks for more contrast (macOS's and iOS's Increase Contrast,
    /// [`rune::Cx::system_increased_contrast`]): muted and faint text and icons move halfway to
    /// the base ink, and borders move a third of the way to it, more opaque as they go. A text
    /// role still at the colour [`TextRoles::from_palette`] gives it follows its token (so `meta`
    /// lifts with faint text) and reads at 7:1 on the surfaces text sits on; a role the theme or
    /// app set keeps its colour, so one already picked for increased contrast
    /// ([`RoleColor`](crate::RoleColor)) is not lifted again. Code then reads at 7:1 on what it
    /// sits on, and diffs tint added and removed lines more deeply. A kit already made for
    /// increased contrast comes back unchanged.
    pub fn increased_contrast(&self) -> Kit {
        let mut k = self.clone();
        if k.increased_contrast {
            return k;
        }
        let before = TextRoles::legible_on(&k.c, k.text_floor());
        k.increased_contrast = true;
        let c = &mut k.c;
        c.text_muted = c.text_muted.mix(c.text_base, 0.5);
        c.text_faint = c.text_faint.mix(c.text_base, 0.5);
        c.icon_muted = c.icon_muted.mix(c.icon_base, 0.5);
        c.icon_faint = c.icon_faint.mix(c.icon_base, 0.5);
        c.border_muted = c.border_muted.mix(c.text_base, 0.3);
        c.border_base = c.border_base.mix(c.text_base, 0.3);
        c.border_strong = c.border_strong.mix(c.text_base, 0.3);
        let after = TextRoles::legible_on(&k.c, k.text_floor());
        for role in TextRole::ALL {
            if k.roles.get(role) == before.get(role) {
                k.roles.set(role, after.get(role));
            }
        }
        k
    }

    pub(crate) fn text_floor(&self) -> f32 {
        if self.increased_contrast { 7.0 } else { 4.5 }
    }

    pub(crate) fn change_tint(&self, base: Color, hue: Color, t: f32) -> Color {
        let plain = tint(base, hue, t);
        if !self.increased_contrast {
            return plain;
        }
        let deep = tint(base, hue, t * CHANGE_TINT_INCREASED);
        let ink = if self.dark { Color::WHITE } else { Color::BLACK };
        if contrast(ink, deep) >= self.text_floor() * CHANGE_TINT_HEADROOM {
            return deep;
        }
        let [l, .., alpha] = plain.to_oklab();
        let [_, a, b, _] = deep.to_oklab();
        Color::from_oklab([l, a, b, alpha])
    }
}

const CHANGE_TINT_INCREASED: f32 = 1.6;
const CHANGE_TINT_HEADROOM: f32 = 1.5;

#[cfg(test)]
#[path = "contrast_tests.rs"]
mod tests;
