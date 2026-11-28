use rune::Color;

use super::kit::Kit;
use super::palette::{contrast, legible, tint};
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

    /// The kit with the text colours an app set held to the contrast the kit's own text keeps,
    /// on the surfaces text sits on (`bg_base`, `bg_deep` and `layer_01`): `text_strong`,
    /// `text_base`, `text_weak`, `text_muted`, `text_accent` and every [`TextRole`] at 4.5:1,
    /// and `text_faint` at 3:1; 7:1 and 4.5:1 on a kit made for
    /// [`increased_contrast`](Kit::increased_contrast). A colour that already reads at its floor
    /// is left as it is; one that falls short becomes opaque and moves toward black or white
    /// along its hue ([`legible`](crate::legible)) until it does. Icons, borders, fills and the
    /// text drawn on fills (`text_inverse`, `text_contrast`) are left alone.
    ///
    /// Call it after overriding the colours, and after [`Kit::increased_contrast`], which keeps
    /// roles it did not derive as they are.
    ///
    /// ```
    /// use rune::color::hex;
    /// use rune_kit::Kit;
    /// let mut kit = Kit::neutral(false);
    /// kit.c.text_base = hex("#9a9a9a").unwrap();
    /// kit.roles.link = hex("#7aa7ff").unwrap();
    /// let kit = kit.legible_text();
    /// assert_ne!(kit.c.text_base, hex("#9a9a9a").unwrap());
    /// ```
    pub fn legible_text(&self) -> Kit {
        let mut k = self.clone();
        let (body, faint) = if k.increased_contrast { (7.0, 4.5) } else { (4.5, 3.0) };
        let surfaces = [k.c.bg_base, k.c.bg_deep, k.c.layer_01];
        let floor = |color: Color, min: f32| {
            if surfaces.iter().all(|&s| contrast(s.mix(color.alpha(1.0), color.a), s) >= min) {
                return color;
            }
            surfaces.iter().fold(color, |color, &s| legible(color, s, min))
        };
        let c = &mut k.c;
        for token in [&mut c.text_strong, &mut c.text_base, &mut c.text_weak, &mut c.text_muted, &mut c.text_accent] {
            *token = floor(*token, body);
        }
        c.text_faint = floor(c.text_faint, faint);
        for role in TextRole::ALL {
            k.roles.set(role, floor(k.roles.get(role), body));
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
