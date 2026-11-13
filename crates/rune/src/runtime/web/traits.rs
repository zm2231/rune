//! The focused field's keyboard traits as the page's hidden text field takes them: the HTML
//! attribute values of `inputmode`, `autocomplete`, `autocapitalize`, `autocorrect`,
//! `spellcheck` and `enterkeyhint`.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use crate::input::traits::InputTraits;
use crate::{Autocapitalize, Autocomplete, EnterKeyHint, InputMode};

/// One attribute value each. An empty `autocorrect`, `spellcheck` or `enterkeyhint` takes the
/// attribute off, which leaves the choice to the browser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Tokens {
    pub(crate) input_mode: &'static str,
    pub(crate) autocomplete: &'static str,
    pub(crate) autocapitalize: &'static str,
    pub(crate) autocorrect: &'static str,
    pub(crate) spellcheck: &'static str,
    pub(crate) enter_key_hint: &'static str,
}

impl Tokens {
    pub(crate) fn of(traits: &InputTraits) -> Tokens {
        Tokens {
            input_mode: match traits.mode {
                InputMode::Text => "text",
                InputMode::Decimal => "decimal",
                InputMode::Numeric => "numeric",
                InputMode::Tel => "tel",
                InputMode::Search => "search",
                InputMode::Email => "email",
                InputMode::Url => "url",
            },
            autocomplete: match traits.autocomplete {
                Autocomplete::Off => "off",
                Autocomplete::Username => "username",
                Autocomplete::CurrentPassword => "current-password",
                Autocomplete::NewPassword => "new-password",
                Autocomplete::OneTimeCode => "one-time-code",
                Autocomplete::Email => "email",
                Autocomplete::Tel => "tel",
                Autocomplete::Url => "url",
                Autocomplete::Name => "name",
                Autocomplete::GivenName => "given-name",
                Autocomplete::FamilyName => "family-name",
                Autocomplete::Organization => "organization",
                Autocomplete::StreetAddress => "street-address",
                Autocomplete::City => "address-level2",
                Autocomplete::Region => "address-level1",
                Autocomplete::PostalCode => "postal-code",
                Autocomplete::CountryName => "country-name",
            },
            autocapitalize: match traits.autocapitalize {
                Autocapitalize::None => "none",
                Autocapitalize::Words => "words",
                Autocapitalize::Sentences => "sentences",
                Autocapitalize::Characters => "characters",
            },
            autocorrect: match traits.autocorrect {
                None => "",
                Some(true) => "on",
                Some(false) => "off",
            },
            spellcheck: match traits.spellcheck {
                None => "",
                Some(true) => "true",
                Some(false) => "false",
            },
            enter_key_hint: match traits.enter_key_hint {
                EnterKeyHint::Enter => "",
                EnterKeyHint::Done => "done",
                EnterKeyHint::Go => "go",
                EnterKeyHint::Next => "next",
                EnterKeyHint::Search => "search",
                EnterKeyHint::Send => "send",
            },
        }
    }
}

impl crate::runtime::RuntimeCore {
    /// The traits for the page's text field, and whether they are a password's. A focused
    /// field with no editor (`El::on_edit`) is not mirrored to the page (`web_context`), so it
    /// counts as no field here too.
    pub(crate) fn web_input_traits(&self) -> (InputTraits, bool) {
        match self.focused_input().filter(|i| i.access.is_some()) {
            Some(input) => (input.traits.resolve(input.edit.is_secure(), input.spellcheck), input.edit.is_secure()),
            None => Default::default(),
        }
    }
}

#[cfg(test)]
#[path = "traits_tests.rs"]
mod tests;
