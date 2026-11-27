use rune::Color;

use super::kit::Palette;
use super::theme::KitEnv;

/// What a run of text means, for the colours [`TextRoles`] gives each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextRole {
    /// Timestamps, counts and other details beside the content.
    Meta,
    Link,
    /// Inline code and paths in running text.
    Code,
    /// Work in progress: a live session, a running task.
    Working,
    /// Lines and words a diff adds.
    Added,
    /// Lines and words a diff removes.
    Removed,
    /// Messages from someone other than the reader.
    Peer,
    /// Something waiting on the reader.
    NeedsAttention,
    Done,
    Error,
}

impl TextRole {
    pub const ALL: [TextRole; 10] = [
        TextRole::Meta,
        TextRole::Link,
        TextRole::Code,
        TextRole::Working,
        TextRole::Added,
        TextRole::Removed,
        TextRole::Peer,
        TextRole::NeedsAttention,
        TextRole::Done,
        TextRole::Error,
    ];

    /// The role's name in theme and settings files: `meta`, `link`, `code`, `working`, `added`,
    /// `removed`, `peer`, `needs_attention`, `done` or `error`.
    pub fn name(self) -> &'static str {
        match self {
            TextRole::Meta => "meta",
            TextRole::Link => "link",
            TextRole::Code => "code",
            TextRole::Working => "working",
            TextRole::Added => "added",
            TextRole::Removed => "removed",
            TextRole::Peer => "peer",
            TextRole::NeedsAttention => "needs_attention",
            TextRole::Done => "done",
            TextRole::Error => "error",
        }
    }

    /// The role [`Self::name`] gives `name`, or None.
    pub fn named(name: &str) -> Option<TextRole> {
        TextRole::ALL.into_iter().find(|r| r.name() == name)
    }
}

/// The text colour of each [`TextRole`] for one appearance, on [`Kit::roles`](crate::Kit::roles).
/// The markdown view's links and diffs and the diff view's added and removed lines take theirs
/// from here; an app colours its own timestamps, live work or peers' messages from the rest.
#[derive(Clone, Copy, Debug, PartialEq, Hash)]
pub struct TextRoles {
    pub meta: Color,
    pub link: Color,
    pub code: Color,
    pub working: Color,
    pub added: Color,
    pub removed: Color,
    pub peer: Color,
    pub needs_attention: Color,
    pub done: Color,
    pub error: Color,
}

impl TextRoles {
    /// Roles from a palette's own tokens: `text_faint` for meta, `text_accent` for links and
    /// work in progress, `text_base` for code and peers, the success and danger icon hues for
    /// added and removed, and the warning, success and danger text for the states.
    pub fn from_palette(c: &Palette) -> TextRoles {
        TextRoles {
            meta: c.text_faint,
            link: c.text_accent,
            code: c.text_base,
            working: c.text_accent,
            added: c.success.icon,
            removed: c.danger.icon,
            peer: c.text_base,
            needs_attention: c.warning.fg,
            done: c.success.fg,
            error: c.danger.fg,
        }
    }

    pub fn get(&self, role: TextRole) -> Color {
        *self.slot(role)
    }

    pub fn set(&mut self, role: TextRole, color: Color) {
        *self.slot_mut(role) = color;
    }

    fn slot(&self, role: TextRole) -> &Color {
        match role {
            TextRole::Meta => &self.meta,
            TextRole::Link => &self.link,
            TextRole::Code => &self.code,
            TextRole::Working => &self.working,
            TextRole::Added => &self.added,
            TextRole::Removed => &self.removed,
            TextRole::Peer => &self.peer,
            TextRole::NeedsAttention => &self.needs_attention,
            TextRole::Done => &self.done,
            TextRole::Error => &self.error,
        }
    }

    fn slot_mut(&mut self, role: TextRole) -> &mut Color {
        match role {
            TextRole::Meta => &mut self.meta,
            TextRole::Link => &mut self.link,
            TextRole::Code => &mut self.code,
            TextRole::Working => &mut self.working,
            TextRole::Added => &mut self.added,
            TextRole::Removed => &mut self.removed,
            TextRole::Peer => &mut self.peer,
            TextRole::NeedsAttention => &mut self.needs_attention,
            TextRole::Done => &mut self.done,
            TextRole::Error => &mut self.error,
        }
    }
}

/// One role's colour in each appearance, and optionally the colours to use when the system asks
/// for more contrast, as an app's settings give them. [`Self::pick`] chooses for a window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoleColor {
    pub dark: Color,
    pub light: Color,
    /// (dark, light) under increased contrast; `dark` and `light` when None.
    pub increased: Option<(Color, Color)>,
}

impl RoleColor {
    pub fn new(dark: Color, light: Color) -> RoleColor {
        RoleColor { dark, light, increased: None }
    }

    /// This pair with (dark, light) colours for increased contrast.
    pub fn increased(mut self, dark: Color, light: Color) -> RoleColor {
        self.increased = Some((dark, light));
        self
    }

    /// The colour for `env`'s appearance and contrast.
    pub fn pick(&self, env: &KitEnv) -> Color {
        let (dark, light) = match self.increased {
            Some(pair) if env.increased_contrast => pair,
            _ => (self.dark, self.light),
        };
        if env.dark { dark } else { light }
    }
}

#[cfg(test)]
#[path = "roles_tests.rs"]
mod tests;
