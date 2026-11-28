use rune::Color;

use super::kit::Kit;
use super::kit::Palette;
use super::legible::{legible_on_all, lowest_contrast};
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

    /// The kit with the text colours an app set held to the contrast the kit's own text keeps,
    /// on the surfaces text sits on (`bg_base`, `bg_deep` and `layer_01`): `text_strong`,
    /// `text_base`, `text_weak`, `text_muted`, `text_accent` and every [`TextRole`] at 4.5:1,
    /// and `text_faint` at 3:1; 7:1 and 4.5:1 on a kit made for
    /// [`increased_contrast`](Kit::increased_contrast). Each colour is held on all three
    /// surfaces at once ([`legible_on_all`](crate::legible_on_all)): one that already reads on
    /// all of them is left as it is; one that falls short becomes opaque and moves toward black
    /// or white along its hue until it reads on every one. Icons, borders, fills and the text
    /// drawn on fills (`text_inverse`, `text_contrast`) are left alone.
    ///
    /// Surfaces that mix light and dark can leave no colour that reads at the floor on all of
    /// them; such a colour gets the most contrast it can, and
    /// [`text_short_of_floor`](Kit::text_short_of_floor) names it.
    ///
    /// Call it after overriding the colours, and after [`Kit::increased_contrast`], which keeps
    /// roles it did not derive as they are. Translucent colours are measured in the blend mode
    /// [`legible`](crate::legible) describes: a kit built outside its window's view or its
    /// [`KitEnv`](crate::KitEnv) for a linear-light window calls this inside [`rune::theme::with_blend`].
    ///
    /// ```
    /// use rune::color::hex;
    /// use rune_kit::Kit;
    /// let mut kit = Kit::neutral(false);
    /// kit.c.text_base = hex("#9a9a9a").unwrap();
    /// kit.roles.link = hex("#7aa7ff").unwrap();
    /// let kit = kit.legible_text();
    /// assert_ne!(kit.c.text_base, hex("#9a9a9a").unwrap());
    /// assert!(kit.text_short_of_floor().is_empty());
    /// ```
    pub fn legible_text(&self) -> Kit {
        let mut k = self.clone();
        let (body, faint) = k.text_floors();
        let surfaces = k.c.text_surfaces();
        let c = &mut k.c;
        for token in [&mut c.text_strong, &mut c.text_base, &mut c.text_weak, &mut c.text_muted, &mut c.text_accent] {
            *token = legible_on_all(*token, &surfaces, body);
        }
        c.text_faint = legible_on_all(c.text_faint, &surfaces, faint);
        for role in TextRole::ALL {
            k.roles.set(role, legible_on_all(k.roles.get(role), &surfaces, body));
        }
        k
    }

    /// The text colours [`legible_text`](Kit::legible_text) holds that read under their floor
    /// on `bg_base`, `bg_deep` or `layer_01`, in the order that method lists them. After
    /// `legible_text`, a colour is here only when no colour reaches its floor on all three
    /// surfaces at once: the surfaces mix light and dark too far apart for it.
    ///
    /// ```
    /// use rune::{Color, color::hex};
    /// use rune_kit::Kit;
    /// let mut kit = Kit::neutral(true);
    /// kit.c.bg_base = hex("#1e1e1e").unwrap();
    /// kit.c.layer_01 = hex("#f0f0f0").unwrap();
    /// let short = kit.legible_text().text_short_of_floor();
    /// assert!(short.iter().any(|s| s.name == "text_base" && s.lowest < s.floor));
    /// ```
    pub fn text_short_of_floor(&self) -> Vec<TextShortfall> {
        let (body, faint) = self.text_floors();
        let surfaces = self.c.text_surfaces();
        let c = &self.c;
        let tokens = [
            ("text_strong", c.text_strong, body),
            ("text_base", c.text_base, body),
            ("text_weak", c.text_weak, body),
            ("text_muted", c.text_muted, body),
            ("text_accent", c.text_accent, body),
            ("text_faint", c.text_faint, faint),
        ];
        let roles = TextRole::ALL.map(|role| (role.name(), self.roles.get(role), body));
        tokens
            .into_iter()
            .chain(roles)
            .filter_map(|(name, color, floor)| {
                let lowest = lowest_contrast(color, &surfaces);
                (lowest < floor).then_some(TextShortfall { name, color, lowest, floor })
            })
            .collect()
    }

    fn text_floors(&self) -> (f32, f32) {
        if self.increased_contrast { (7.0, 4.5) } else { (4.5, 3.0) }
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

/// A text colour on a [`Kit`] that reads under its contrast floor on one of the surfaces text
/// sits on, from [`Kit::text_short_of_floor`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextShortfall {
    /// The token (`text_base`, `text_faint`, ...) or [`TextRole::name`].
    pub name: &'static str,
    /// Its colour.
    pub color: Color,
    /// Its lowest contrast on `bg_base`, `bg_deep` and `layer_01`.
    pub lowest: f32,
    /// The contrast it is held to.
    pub floor: f32,
}

impl Palette {
    pub(crate) fn text_surfaces(&self) -> [Color; 3] {
        [self.bg_base, self.bg_deep, self.layer_01]
    }
}

const CHANGE_TINT_INCREASED: f32 = 1.6;
const CHANGE_TINT_HEADROOM: f32 = 1.5;

#[cfg(test)]
#[path = "contrast_tests.rs"]
mod tests;
