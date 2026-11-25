//! What a text field tells a platform's own text system: which keyboard to show, what its
//! return key says, and whether to correct as the person types. Rune's own editing ignores them;
//! a host with a software keyboard reads them through [`Runtime::host_text`](crate::Runtime::host_text).

use super::*;

/// The software keyboard a text field asks for ([`El::keyboard`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Keyboard {
    /// Letters, in the person's own layout.
    #[default]
    Text,
    /// An email address: `@` and `.` on the first layer, no autocorrect.
    Email,
    /// A web address: `/` and `.com`, no autocorrect.
    Url,
    /// Digits only.
    Number,
    /// Digits and the locale's decimal separator.
    Decimal,
    /// A phone number's digits, `*`, `#` and `+`.
    Phone,
    /// Letters, for a query: the return key reads Search.
    Search,
}

/// What a software keyboard's return key says ([`El::return_key`]). Pressing it is Return
/// either way: a single-line field submits, a paragraph breaks or submits as
/// [`text_input`] describes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ReturnKey {
    /// The platform's own: "return", or Search for [`Keyboard::Search`].
    #[default]
    Default,
    Go,
    Next,
    Search,
    Send,
    Done,
}

/// A text field's hints for a platform text system, as [`HostText::hints`](crate::HostText)
/// reports them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct TextHints {
    pub keyboard: Keyboard,
    pub return_key: ReturnKey,
    /// Whether the platform may correct and predict words. Off for a secure field and for
    /// [`Keyboard::Email`], [`Keyboard::Url`] and the digit keyboards unless [`El::autocorrect`]
    /// turned it on.
    pub autocorrect: bool,
}

/// [`El::autocorrect`]'s setting before a field's keyboard and security settle it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct HintSpec {
    pub(crate) keyboard: Keyboard,
    pub(crate) return_key: ReturnKey,
    pub(crate) autocorrect: Option<bool>,
}

impl HintSpec {
    /// The hints a host sees: a secure field never corrects, and an address or number corrects
    /// only when asked to.
    pub(crate) fn resolve(self, secure: bool) -> TextHints {
        let words = matches!(self.keyboard, Keyboard::Text | Keyboard::Search);
        TextHints {
            keyboard: self.keyboard,
            return_key: self.return_key,
            autocorrect: !secure && self.autocorrect.unwrap_or(words),
        }
    }
}

impl El {
    /// The software keyboard this text field asks for. Honoured by the iOS host (`rune-ios`);
    /// desktop hosts have no software keyboard, and the web host ignores it for now.
    pub fn keyboard(mut self, keyboard: Keyboard) -> Self {
        if let Some(i) = self.cold.get_mut_if_some().and_then(|c| c.input.as_deref_mut()) {
            i.hints.keyboard = keyboard;
        }
        self
    }

    /// What this text field's software keyboard return key says. Honoured by the iOS host
    /// (`rune-ios`); the web host ignores it for now.
    pub fn return_key(mut self, key: ReturnKey) -> Self {
        if let Some(i) = self.cold.get_mut_if_some().and_then(|c| c.input.as_deref_mut()) {
            i.hints.return_key = key;
        }
        self
    }

    /// Whether the platform's keyboard may autocorrect and predict words in this text field. On
    /// by default for [`Keyboard::Text`] and [`Keyboard::Search`], off for the others; a secure
    /// field never corrects. Honoured by the iOS host (`rune-ios`); the web host ignores it for
    /// now.
    pub fn autocorrect(mut self, on: bool) -> Self {
        if let Some(i) = self.cold.get_mut_if_some().and_then(|c| c.input.as_deref_mut()) {
            i.hints.autocorrect = Some(on);
        }
        self
    }
}
