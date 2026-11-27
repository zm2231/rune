use super::kit::Kit;

impl Kit {
    /// The kit for a system that asks for more contrast (macOS's and iOS's Increase Contrast,
    /// [`rune::Cx::system_increased_contrast`]): muted and faint text and icons move halfway to
    /// the base ink, as does the [`meta`](crate::TextRoles::meta) role, and borders move a third
    /// of the way to it, more opaque as they go.
    pub fn increased_contrast(&self) -> Kit {
        let mut k = self.clone();
        let c = &mut k.c;
        c.text_muted = c.text_muted.mix(c.text_base, 0.5);
        c.text_faint = c.text_faint.mix(c.text_base, 0.5);
        c.icon_muted = c.icon_muted.mix(c.icon_base, 0.5);
        c.icon_faint = c.icon_faint.mix(c.icon_base, 0.5);
        c.border_muted = c.border_muted.mix(c.text_base, 0.3);
        c.border_base = c.border_base.mix(c.text_base, 0.3);
        c.border_strong = c.border_strong.mix(c.text_base, 0.3);
        k.roles.meta = k.roles.meta.mix(k.c.text_base, 0.5);
        k
    }
}

#[cfg(test)]
#[path = "contrast_tests.rs"]
mod tests;
