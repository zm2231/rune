use super::*;

const PLAIN: Traits = (InputTraits { mode: InputMode::Text, autocomplete: Autocomplete::Off,
    autocapitalize: Autocapitalize::Sentences, autocorrect: None, spellcheck: None, enter_key_hint: EnterKeyHint::Enter }, false);
const DIGITS: Traits = (InputTraits { mode: InputMode::Numeric, ..PLAIN.0 }, false);
const PASSWORD: Traits = (PLAIN.0, true);

fn frame(asked: Traits, composing: bool) -> Frame { Frame { asked, attached: true, composing } }

fn reloads(state: &mut Reload, frame: Frame) -> usize { state.step(frame).into_iter().count() }

#[test]
fn a_change_under_an_attached_keyboard_reloads_it_once() {
    let mut state = Reload::default();
    assert_eq!(reloads(&mut state, frame(PLAIN, false)), 0, "nothing changed");
    assert_eq!(reloads(&mut state, frame(DIGITS, false)), 1, "the focus moved to a field with another keyboard");
    assert_eq!(reloads(&mut state, frame(DIGITS, false)), 0);
    assert_eq!(reloads(&mut state, frame(PASSWORD, false)), 1, "a field turning secure goes the same way");
    assert_eq!(state, Reload::Read(PASSWORD));
}

#[test]
fn a_keyboard_that_is_not_attached_reads_the_traits_when_it_attaches() {
    let mut state = Reload::default();
    let away = |asked| Frame { asked, attached: false, composing: false };
    assert_eq!(reloads(&mut state, away(DIGITS)), 0, "this frame attaches it, or no field has the focus");
    assert_eq!(reloads(&mut state, frame(DIGITS, false)), 0, "it read them as it attached");
    // A change it was waiting to show is dropped with the keyboard.
    assert_eq!(reloads(&mut state, frame(PASSWORD, true)), 0);
    assert_eq!(reloads(&mut state, away(PASSWORD)), 0);
    assert_eq!(reloads(&mut state, frame(PASSWORD, false)), 0);
}

#[test]
fn a_change_during_a_composition_waits_for_it_to_end_then_reloads_once() {
    let mut state = Reload::Read(PLAIN);
    for _ in 0..3 {
        assert_eq!(reloads(&mut state, frame(DIGITS, true)), 0, "reloading would end the composition");
        assert_eq!(state, Reload::Held { read: PLAIN });
    }
    assert_eq!(reloads(&mut state, frame(DIGITS, false)), 1, "the composition ended");
    assert_eq!(reloads(&mut state, frame(DIGITS, false)), 0, "exactly one");
    assert_eq!(state, Reload::Read(DIGITS));
}

#[test]
fn a_change_taken_back_during_the_composition_reloads_nothing() {
    let mut state = Reload::Read(PLAIN);
    assert_eq!(reloads(&mut state, frame(PASSWORD, true)), 0);
    assert_eq!(reloads(&mut state, frame(PLAIN, true)), 0);
    assert_eq!(state, Reload::Read(PLAIN), "the keyboard shows what is asked");
    assert_eq!(reloads(&mut state, frame(PLAIN, false)), 0);
}

/// Each raw value as `UITextInputTraits.h` counts its enum from zero.
#[test]
fn keyboards_are_uikits_raw_values() {
    use InputMode::*;
    // Default, ASCIICapable, NumbersAndPunctuation, URL, NumberPad, PhonePad, NamePhonePad,
    // EmailAddress, DecimalPad, Twitter, WebSearch.
    let raw = [(Text, 0), (Url, 3), (Numeric, 4), (Tel, 5), (Email, 7), (Decimal, 8), (Search, 10)];
    for (mode, value) in raw { assert_eq!(keyboard_type(mode), value, "{mode:?}"); }
}

#[test]
fn capitalization_is_uikits_raw_values() {
    use Autocapitalize::*;
    // None, Words, Sentences, AllCharacters.
    for (how, value) in [(None, 0), (Words, 1), (Sentences, 2), (Characters, 3)] {
        assert_eq!(autocapitalization_type(how), value, "{how:?}");
    }
    assert_eq!(autocapitalization_type(Autocapitalize::default()), 2, "UIKit's own default is sentences");
}

#[test]
fn correction_and_spell_checking_are_default_no_yes() {
    assert_eq!([correction_type(None), correction_type(Some(false)), correction_type(Some(true))], [0, 1, 2]);
}

#[test]
fn return_keys_are_uikits_raw_values() {
    use EnterKeyHint::*;
    // Default, Go, Google, Join, Next, Route, Search, Send, Yahoo, Done.
    for (hint, value) in [(Enter, 0), (Go, 1), (Next, 4), (Search, 6), (Send, 7), (Done, 9)] {
        assert_eq!(return_key_type(hint), value, "{hint:?}");
    }
}

#[test]
fn every_autofill_kind_has_its_content_type_and_off_has_none() {
    use Autocomplete as A;
    use ContentType as C;
    let table = [
        (A::Username, C::Username), (A::CurrentPassword, C::Password), (A::NewPassword, C::NewPassword),
        (A::OneTimeCode, C::OneTimeCode), (A::Email, C::EmailAddress), (A::Tel, C::TelephoneNumber), (A::Url, C::URL),
        (A::Name, C::Name), (A::GivenName, C::GivenName), (A::FamilyName, C::FamilyName),
        (A::Organization, C::OrganizationName), (A::StreetAddress, C::FullStreetAddress), (A::City, C::AddressCity),
        (A::Region, C::AddressState), (A::PostalCode, C::PostalCode), (A::CountryName, C::CountryName),
    ];
    for (kind, content) in table { assert_eq!(content_type(kind), Some(content), "{kind:?}"); }
    assert_eq!(content_type(A::Off), None);
}

/// A field that asked for nothing answers UIKit as a responder that answers nothing does.
#[test]
fn an_untouched_field_is_uikits_defaults() {
    let traits = InputTraits::default();
    assert_eq!(keyboard_type(traits.mode), 0);
    assert_eq!(content_type(traits.autocomplete), None);
    assert_eq!(autocapitalization_type(traits.autocapitalize), 2);
    assert_eq!((correction_type(traits.autocorrect), correction_type(traits.spellcheck)), (0, 0));
    assert_eq!(return_key_type(traits.enter_key_hint), 0);
}
