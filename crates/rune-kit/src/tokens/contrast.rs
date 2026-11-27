use super::kit::Kit;
use super::roles::{TextRole, TextRoles};

impl Kit {
    /// The kit for a system that asks for more contrast (macOS's and iOS's Increase Contrast,
    /// [`rune::Cx::system_increased_contrast`]): muted and faint text and icons move halfway to
    /// the base ink, and borders move a third of the way to it, more opaque as they go. A text
    /// role still at the colour [`TextRoles::from_palette`] gives it follows its token (so `meta`
    /// lifts with faint text); a role the theme or app set keeps its colour, so one already picked
    /// for increased contrast ([`RoleColor`](crate::RoleColor)) is not lifted again.
    pub fn increased_contrast(&self) -> Kit {
        let mut k = self.clone();
        let before = TextRoles::from_palette(&k.c);
        let c = &mut k.c;
        c.text_muted = c.text_muted.mix(c.text_base, 0.5);
        c.text_faint = c.text_faint.mix(c.text_base, 0.5);
        c.icon_muted = c.icon_muted.mix(c.icon_base, 0.5);
        c.icon_faint = c.icon_faint.mix(c.icon_base, 0.5);
        c.border_muted = c.border_muted.mix(c.text_base, 0.3);
        c.border_base = c.border_base.mix(c.text_base, 0.3);
        c.border_strong = c.border_strong.mix(c.text_base, 0.3);
        let after = TextRoles::from_palette(&k.c);
        for role in TextRole::ALL {
            if k.roles.get(role) == before.get(role) {
                k.roles.set(role, after.get(role));
            }
        }
        k
    }
}

#[cfg(test)]
#[path = "contrast_tests.rs"]
mod tests;
