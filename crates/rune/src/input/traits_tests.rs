use super::*;

use Autocapitalize as Cap;
use Autocomplete as Fill;

fn asked() -> Asked { Asked::default() }

/// Mode, capitalization and autocorrection, the three a rule can change.
fn keyboard(t: InputTraits) -> (InputMode, Cap, Option<bool>) { (t.mode, t.autocapitalize, t.autocorrect) }

#[test]
fn a_field_that_asked_for_nothing_gets_the_systems_defaults() {
    let traits = asked().resolve(false, false);
    assert_eq!(traits, InputTraits::default(), "an app-drawn editor's traits are an untouched field's");
    assert_eq!(traits, InputTraits {
        mode: InputMode::Text, autocomplete: Fill::Off, autocapitalize: Cap::Sentences,
        autocorrect: None, spellcheck: None, enter_key_hint: EnterKeyHint::Enter,
    });
    assert!(std::mem::size_of::<InputTraits>() <= 8, "a frame compares it by value");
}

#[test]
fn every_autocomplete_kind_brings_its_keyboard_and_capitals_and_turns_correction_off() {
    use InputMode::*;
    let table = [
        (Fill::Username, Text, Cap::None), (Fill::CurrentPassword, Text, Cap::None), (Fill::NewPassword, Text, Cap::None),
        (Fill::OneTimeCode, Numeric, Cap::None), (Fill::Email, Email, Cap::None), (Fill::Tel, Tel, Cap::None),
        (Fill::Url, Url, Cap::None), (Fill::Name, Text, Cap::Words), (Fill::GivenName, Text, Cap::Words),
        (Fill::FamilyName, Text, Cap::Words), (Fill::Organization, Text, Cap::Words),
        (Fill::StreetAddress, Text, Cap::Words), (Fill::City, Text, Cap::Words), (Fill::Region, Text, Cap::Words),
        (Fill::PostalCode, Text, Cap::Characters), (Fill::CountryName, Text, Cap::Words),
    ];
    for (kind, mode, capitals) in table {
        let traits = Asked { autocomplete: kind, ..asked() }.resolve(false, false);
        assert_eq!(keyboard(traits), (mode, capitals, Some(false)), "{kind:?}");
        assert_eq!((traits.autocomplete, traits.spellcheck), (kind, Some(false)), "{kind:?}");
    }
    assert_eq!(keyboard(Asked { autocomplete: Fill::Off, ..asked() }.resolve(false, false)), (Text, Cap::Sentences, None));
}

#[test]
fn a_keyboard_for_something_other_than_prose_drops_capitals_and_correction() {
    use InputMode::*;
    for mode in [Decimal, Numeric, Tel, Email, Url] {
        assert_eq!(keyboard(Asked { mode: Some(mode), ..asked() }.resolve(false, false)), (mode, Cap::None, Some(false)));
    }
    for mode in [Text, Search] {
        assert_eq!(keyboard(Asked { mode: Some(mode), ..asked() }.resolve(false, false)), (mode, Cap::Sentences, None));
    }
}

#[test]
fn what_the_field_set_wins_over_what_its_kind_and_keyboard_bring() {
    let email = Asked { autocomplete: Fill::Email, ..asked() };
    assert_eq!(keyboard(Asked { mode: Some(InputMode::Text), ..email }.resolve(false, false)),
        (InputMode::Text, Cap::None, Some(false)), "the kind still sets capitals and correction");
    assert_eq!(keyboard(Asked { autocapitalize: Some(Cap::Words), autocorrect: Some(true), ..email }.resolve(false, false)),
        (InputMode::Email, Cap::Words, Some(true)));
    // The kind's capitals win over the keyboard's.
    let name_on_email_keys = Asked { autocomplete: Fill::Name, mode: Some(InputMode::Email), ..asked() };
    assert_eq!(keyboard(name_on_email_keys.resolve(false, false)), (InputMode::Email, Cap::Words, Some(false)));
    let digits = Asked { mode: Some(InputMode::Numeric), autocapitalize: Some(Cap::Characters), autocorrect: Some(true), ..asked() };
    assert_eq!(keyboard(digits.resolve(false, false)), (InputMode::Numeric, Cap::Characters, Some(true)));
    assert_eq!(keyboard(Asked { autocorrect: Some(false), ..asked() }.resolve(false, false)), (InputMode::Text, Cap::Sentences, Some(false)));
}

#[test]
fn spell_checking_follows_the_fields_own_and_autocorrection() {
    assert_eq!(asked().resolve(false, false).spellcheck, None);
    assert_eq!(asked().resolve(false, true).spellcheck, Some(true));
    assert_eq!(Asked { autocorrect: Some(false), ..asked() }.resolve(false, false).spellcheck, Some(false));
    assert_eq!(Asked { autocorrect: Some(false), ..asked() }.resolve(false, true).spellcheck, Some(true), "the field's own wins");
    assert_eq!(Asked { autocorrect: Some(true), ..asked() }.resolve(false, false).spellcheck, None);
}

#[test]
fn a_secure_field_is_a_password_whatever_it_asked() {
    let password = InputTraits {
        mode: InputMode::Text, autocomplete: Fill::CurrentPassword, autocapitalize: Cap::None,
        autocorrect: Some(false), spellcheck: Some(false), enter_key_hint: EnterKeyHint::Enter,
    };
    assert_eq!(asked().resolve(true, false), password);
    let loud = Asked {
        mode: Some(InputMode::Email), autocomplete: Fill::Email, autocapitalize: Some(Cap::Characters),
        autocorrect: Some(true), enter_key_hint: EnterKeyHint::Enter,
    };
    assert_eq!(loud.resolve(true, true), password, "what it set is ignored, spell checking too");
    assert_eq!(Asked { autocomplete: Fill::NewPassword, ..asked() }.resolve(true, false),
        InputTraits { autocomplete: Fill::NewPassword, ..password });
    assert_eq!(Asked { autocomplete: Fill::OneTimeCode, ..asked() }.resolve(true, false), password,
        "a kind's keyboard does not survive, only one the field set");
    assert_eq!(Asked { mode: Some(InputMode::Numeric), ..asked() }.resolve(true, false),
        InputTraits { mode: InputMode::Numeric, ..password }, "a PIN keeps its digits");
    assert_eq!(Asked { mode: Some(InputMode::Decimal), ..asked() }.resolve(true, false), password);
    assert_eq!(Asked { enter_key_hint: EnterKeyHint::Go, ..asked() }.resolve(true, false),
        InputTraits { enter_key_hint: EnterKeyHint::Go, ..password }, "the Return caption is kept");
}

#[test]
fn the_return_caption_is_what_the_field_set() {
    use EnterKeyHint::*;
    for hint in [Enter, Done, Go, Next, Search, Send] {
        assert_eq!(Asked { enter_key_hint: hint, autocomplete: Fill::Email, ..asked() }.resolve(false, false).enter_key_hint, hint);
    }
}
