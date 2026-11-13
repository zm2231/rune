//! The focused field's keyboard traits as UIKit's `UITextInputTraits` takes them: the raw value
//! of each enum, and which `UITextContentType` an autofill kind is. The responder (`input`)
//! answers UIKit with these; they are here so a host build can check every value.
//!
//! The keyboard reads the traits when it attaches, so a change under an attached keyboard
//! reloads it ([`Reload`]). Reloading ends a composition, so the reload waits for one to end.

use crate::input::traits::InputTraits;
use crate::machine::Effects;
use crate::{Autocapitalize, Autocomplete, EnterKeyHint, InputMode};

/// What the keyboard reads from the responder: the field's traits and whether it is a password.
pub(crate) type Traits = (InputTraits, bool);

/// Whether the attached keyboard has read what the focused field asks of it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Reload {
    /// No keyboard is attached, or it read these when it attached or was last reloaded.
    Read(Traits),
    /// It read `read` and the field has asked for something else since, while the keyboard
    /// was composing: it reloads when the composition ends.
    Held { read: Traits },
}

impl Default for Reload {
    fn default() -> Self { Reload::Read(Traits::default()) }
}

/// A frame's news for the keyboard.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Frame {
    /// What the focused field asks now.
    pub(crate) asked: Traits,
    /// The responder was first responder before this frame and stays so. A keyboard that
    /// attaches this frame reads `asked` as it does.
    pub(crate) attached: bool,
    /// The keyboard has marked text open.
    pub(crate) composing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReloadEffect {
    /// `reloadInputViews`: the keyboard reads the traits again.
    Reload,
}

impl Reload {
    pub(crate) fn step(&mut self, frame: Frame) -> Effects<ReloadEffect> {
        let read = match *self {
            Reload::Read(read) | Reload::Held { read } => read,
        };
        if !frame.attached || read == frame.asked {
            // Asking again for what the keyboard shows leaves nothing to reload.
            *self = Reload::Read(frame.asked);
            Effects::none()
        } else if frame.composing {
            *self = Reload::Held { read };
            Effects::none()
        } else {
            *self = Reload::Read(frame.asked);
            Effects::one(ReloadEffect::Reload)
        }
    }
}

/// `UIKeyboardType`.
pub(crate) fn keyboard_type(mode: InputMode) -> isize {
    match mode {
        InputMode::Text => 0,     // UIKeyboardTypeDefault
        InputMode::Url => 3,      // UIKeyboardTypeURL
        InputMode::Numeric => 4,  // UIKeyboardTypeNumberPad
        InputMode::Tel => 5,      // UIKeyboardTypePhonePad
        InputMode::Email => 7,    // UIKeyboardTypeEmailAddress
        InputMode::Decimal => 8,  // UIKeyboardTypeDecimalPad
        InputMode::Search => 10,  // UIKeyboardTypeWebSearch
    }
}

/// `UITextAutocapitalizationType`.
pub(crate) fn autocapitalization_type(how: Autocapitalize) -> isize {
    match how {
        Autocapitalize::None => 0,
        Autocapitalize::Words => 1,
        Autocapitalize::Sentences => 2,
        Autocapitalize::Characters => 3,  // UITextAutocapitalizationTypeAllCharacters
    }
}

/// `UITextAutocorrectionType` and `UITextSpellCheckingType`, which count alike: the system's
/// default, no, yes.
pub(crate) fn correction_type(on: Option<bool>) -> isize {
    match on {
        None => 0,
        Some(false) => 1,
        Some(true) => 2,
    }
}

/// `UIReturnKeyType`.
pub(crate) fn return_key_type(hint: EnterKeyHint) -> isize {
    match hint {
        EnterKeyHint::Enter => 0,  // UIReturnKeyDefault
        EnterKeyHint::Go => 1,
        EnterKeyHint::Next => 4,
        EnterKeyHint::Search => 6,
        EnterKeyHint::Send => 7,
        EnterKeyHint::Done => 9,
    }
}

/// A `UITextContentType` constant, named as UIKit names it after that prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ContentType {
    Username,
    Password,
    NewPassword,
    OneTimeCode,
    EmailAddress,
    TelephoneNumber,
    URL,
    Name,
    GivenName,
    FamilyName,
    OrganizationName,
    FullStreetAddress,
    AddressCity,
    AddressState,
    PostalCode,
    CountryName,
}

/// What the system may fill the field with. None offers nothing.
pub(crate) fn content_type(kind: Autocomplete) -> Option<ContentType> {
    Some(match kind {
        Autocomplete::Off => return None,
        Autocomplete::Username => ContentType::Username,
        Autocomplete::CurrentPassword => ContentType::Password,
        Autocomplete::NewPassword => ContentType::NewPassword,
        Autocomplete::OneTimeCode => ContentType::OneTimeCode,
        Autocomplete::Email => ContentType::EmailAddress,
        Autocomplete::Tel => ContentType::TelephoneNumber,
        Autocomplete::Url => ContentType::URL,
        Autocomplete::Name => ContentType::Name,
        Autocomplete::GivenName => ContentType::GivenName,
        Autocomplete::FamilyName => ContentType::FamilyName,
        Autocomplete::Organization => ContentType::OrganizationName,
        Autocomplete::StreetAddress => ContentType::FullStreetAddress,
        Autocomplete::City => ContentType::AddressCity,
        Autocomplete::Region => ContentType::AddressState,
        Autocomplete::PostalCode => ContentType::PostalCode,
        Autocomplete::CountryName => ContentType::CountryName,
    })
}

#[cfg(test)]
#[path = "traits_tests.rs"]
mod tests;
