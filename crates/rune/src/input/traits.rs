//! What a text field tells a soft keyboard: which keys to show, what to capitalize and correct,
//! the Return key's caption, and what the system may fill in. The names are the web's
//! (`inputmode`, `autocomplete`, `autocapitalize`, `enterkeyhint`), and iOS splits them the same
//! way. A field says what it wants through `El`'s builders ([`Asked`]); a host reads what that
//! comes to ([`InputTraits`]), so the precedence lives here alone. Desktop platforms have no
//! soft keyboard and read none of it.

/// Which soft keyboard a text field asks for, as the web's `inputmode` does
/// ([`El::input_mode`](crate::El::input_mode)).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum InputMode {
    /// The language's own keyboard.
    #[default]
    Text,
    /// Digits and the locale's decimal separator.
    Decimal,
    /// Digits only: a PIN or a quantity.
    Numeric,
    /// A telephone keypad.
    Tel,
    /// A keyboard for search terms.
    Search,
    /// Letters with `@` and `.` in reach.
    Email,
    /// Letters with `/` and `.` in reach.
    Url,
}

/// What a text field holds, which the system may then offer to fill in, as the web's `autocomplete`
/// does ([`El::autocomplete`](crate::El::autocomplete)). Each kind brings the keyboard and
/// capitalization that suit it and turns autocorrection off; the field's own
/// [`El::input_mode`](crate::El::input_mode), [`El::autocapitalize`](crate::El::autocapitalize)
/// and [`El::autocorrect`](crate::El::autocorrect) win.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Autocomplete {
    /// Names no kind: the system offers what it would for any text.
    #[default]
    Off,
    /// An account's name or handle.
    Username,
    /// The password of an account that exists. A [`secure`](crate::El::secure) field is one
    /// unless it says [`NewPassword`](Self::NewPassword).
    CurrentPassword,
    /// A password being chosen: the system may suggest a strong one.
    NewPassword,
    /// A code sent by message, which the keyboard may offer when it arrives.
    OneTimeCode,
    /// An email address.
    Email,
    /// A full telephone number, with its country code.
    Tel,
    /// A web address.
    Url,
    /// A person's full name.
    Name,
    /// A first name.
    GivenName,
    /// A surname.
    FamilyName,
    /// A company or organization's name.
    Organization,
    /// A street address, on one or more lines.
    StreetAddress,
    /// A city or town (`address-level2`).
    City,
    /// A state, province or region (`address-level1`).
    Region,
    /// A postal or ZIP code.
    PostalCode,
    /// A country's name (`country-name`), not its code.
    CountryName,
}

impl Autocomplete {
    /// The keyboard and capitalization a kind brings. None for [`Off`](Self::Off), which
    /// brings nothing.
    fn implied(self) -> Option<(InputMode, Autocapitalize)> {
        Some(match self {
            Self::Off => return None,
            Self::Username | Self::CurrentPassword | Self::NewPassword => (InputMode::Text, Autocapitalize::None),
            Self::OneTimeCode => (InputMode::Numeric, Autocapitalize::None),
            Self::Email => (InputMode::Email, Autocapitalize::None),
            Self::Tel => (InputMode::Tel, Autocapitalize::None),
            Self::Url => (InputMode::Url, Autocapitalize::None),
            Self::Name | Self::GivenName | Self::FamilyName | Self::Organization | Self::StreetAddress
            | Self::City | Self::Region | Self::CountryName => (InputMode::Text, Autocapitalize::Words),
            Self::PostalCode => (InputMode::Text, Autocapitalize::Characters),
        })
    }
}

/// Which letters the soft keyboard capitalizes on its own, as the web's `autocapitalize` does
/// ([`El::autocapitalize`](crate::El::autocapitalize)).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Autocapitalize {
    /// No letter.
    None,
    /// The first letter of each word.
    Words,
    /// The first letter of each sentence.
    #[default]
    Sentences,
    /// Every letter.
    Characters,
}

/// The Return key's caption on a soft keyboard, as the web's `enterkeyhint` does
/// ([`El::enter_key_hint`](crate::El::enter_key_hint)). The key still does what Return does
/// in the field.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum EnterKeyHint {
    /// The keyboard's own caption.
    #[default]
    Enter,
    /// The field is the last: the keyboard may close.
    Done,
    /// Return goes where the text points: a page, a result.
    Go,
    /// Return moves to the next field.
    Next,
    /// Return runs a search.
    Search,
    /// Return sends the text: a message.
    Send,
}

/// What a text field's builders asked of the keyboard: only what was said, so a later builder
/// or the field turning secure changes what the rest comes to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Asked {
    pub(crate) mode: Option<InputMode>,
    pub(crate) autocomplete: Autocomplete,
    pub(crate) autocapitalize: Option<Autocapitalize>,
    pub(crate) autocorrect: Option<bool>,
    pub(crate) enter_key_hint: EnterKeyHint,
}

impl Asked {
    /// What the keyboard is told. Highest first: a secure field is a password, with no
    /// capitals, correction or spell checking, and keeps only a digits keyboard (a PIN) and its
    /// Return caption; then what the field set; then what its [`Autocomplete`] kind brings;
    /// then, for a keyboard other than `Text` and `Search`, no capitals and no correction; then
    /// the system's defaults. `spellcheck` is [`El::spellcheck`](crate::El::spellcheck).
    pub(crate) fn resolve(self, secure: bool, spellcheck: bool) -> InputTraits {
        if secure {
            return InputTraits {
                mode: if self.mode == Some(InputMode::Numeric) { InputMode::Numeric } else { InputMode::Text },
                autocomplete: if self.autocomplete == Autocomplete::NewPassword { Autocomplete::NewPassword } else { Autocomplete::CurrentPassword },
                autocapitalize: Autocapitalize::None,
                autocorrect: Some(false),
                spellcheck: Some(false),
                enter_key_hint: self.enter_key_hint,
            };
        }
        let implied = self.autocomplete.implied();
        let mode = self.mode.or(implied.map(|(mode, _)| mode)).unwrap_or_default();
        let literal = !matches!(mode, InputMode::Text | InputMode::Search);
        let autocorrect = self.autocorrect.or((implied.is_some() || literal).then_some(false));
        InputTraits {
            mode,
            autocomplete: self.autocomplete,
            autocapitalize: self.autocapitalize.or(implied.map(|(_, capitals)| capitals))
                .or(literal.then_some(Autocapitalize::None)).unwrap_or_default(),
            autocorrect,
            spellcheck: if spellcheck { Some(true) } else { autocorrect.filter(|on| !on) },
            enter_key_hint: self.enter_key_hint,
        }
    }
}

/// What the focused text field tells the soft keyboard, with every rule applied
/// ([`Asked::resolve`]). The default is a field that asked for nothing, which is also what an
/// editor the app draws itself gets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct InputTraits {
    pub(crate) mode: InputMode,
    pub(crate) autocomplete: Autocomplete,
    pub(crate) autocapitalize: Autocapitalize,
    /// Whether the keyboard corrects and predicts words. None leaves it to the system.
    pub(crate) autocorrect: Option<bool>,
    /// Whether the keyboard checks spelling. None leaves it to the system.
    pub(crate) spellcheck: Option<bool>,
    pub(crate) enter_key_hint: EnterKeyHint,
}

#[cfg(test)]
#[path = "traits_tests.rs"]
mod tests;
