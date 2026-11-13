use super::*;
use crate::input::traits::Asked;

fn tokens(asked: Asked) -> Tokens { Tokens::of(&asked.resolve(false, false)) }

#[test]
fn an_untouched_field_adds_only_autocomplete_off_to_a_plain_textarea() {
    // `inputmode=text` and `autocapitalize=sentences` are what the page's textarea is made
    // with; the three empty ones set no attribute.
    assert_eq!(Tokens::of(&InputTraits::default()), Tokens {
        input_mode: "text", autocomplete: "off", autocapitalize: "sentences",
        autocorrect: "", spellcheck: "", enter_key_hint: "",
    });
    assert_eq!(tokens(Asked::default()), Tokens::of(&InputTraits::default()));
}

#[test]
fn only_a_field_with_an_editor_gives_the_page_its_traits() {
    use crate::runtime::Runtime;
    use crate::{Cx, Env};
    fn view(s: &(crate::TextEdit, bool), _: &mut Cx<(crate::TextEdit, bool)>) -> crate::El {
        let field = crate::text_input(&s.0, crate::TextStyle::new(16.0, 400.0)).single_line().key("field")
            .autocomplete(Autocomplete::Email);
        if s.1 { field.on_edit(|s: &mut (crate::TextEdit, bool)| &mut s.0) } else { field }
    }
    let mut rt = Runtime::new((crate::TextEdit::new(""), false), view, (200.0, 40.0), Env::new(true, 1.0));
    rt.frame(0.0);
    let path = rt.tree.nodes.iter().find(|n| n.key.as_deref() == Some("field")).unwrap().path.clone();
    rt.set_focus(Some(path.clone()), true);
    rt.frame(0.0);
    assert!(rt.core().focused_input_traits().is_some());
    assert_eq!(rt.core().web_input_traits(), (InputTraits::default(), false), "the page mirrors no field, so it keeps no traits");
    rt.update(|s, _| s.1 = true);
    rt.frame(0.0);
    rt.set_focus(Some(path), true);
    rt.frame(0.0);
    assert_eq!(rt.core().web_input_traits().0.mode, InputMode::Email);
    rt.update(|s, _| s.0.set_secure(true));
    rt.frame(0.0);
    assert_eq!(rt.core().web_input_traits(), (Asked::default().resolve(true, false), true));
}

#[test]
fn keyboards_are_html_inputmode_keywords() {
    use InputMode::*;
    let names = [(Text, "text"), (Decimal, "decimal"), (Numeric, "numeric"), (Tel, "tel"), (Search, "search"),
        (Email, "email"), (Url, "url")];
    for (mode, name) in names {
        assert_eq!(tokens(Asked { mode: Some(mode), ..Asked::default() }).input_mode, name);
    }
}

#[test]
fn autofill_kinds_are_html_autocomplete_field_names() {
    use Autocomplete::*;
    let names = [(Off, "off"), (Username, "username"), (CurrentPassword, "current-password"),
        (NewPassword, "new-password"), (OneTimeCode, "one-time-code"), (Email, "email"), (Tel, "tel"), (Url, "url"),
        (Name, "name"), (GivenName, "given-name"), (FamilyName, "family-name"), (Organization, "organization"),
        (StreetAddress, "street-address"), (City, "address-level2"), (Region, "address-level1"),
        (PostalCode, "postal-code"), (CountryName, "country-name")];
    for (kind, name) in names {
        assert_eq!(tokens(Asked { autocomplete: kind, ..Asked::default() }).autocomplete, name);
    }
}

#[test]
fn capitals_correction_spelling_and_the_return_caption_are_html_keywords() {
    use Autocapitalize as Cap;
    for (how, name) in [(Cap::None, "none"), (Cap::Words, "words"), (Cap::Sentences, "sentences"), (Cap::Characters, "characters")] {
        assert_eq!(tokens(Asked { autocapitalize: Some(how), ..Asked::default() }).autocapitalize, name);
    }
    let corrected = |on| tokens(Asked { autocorrect: on, ..Asked::default() });
    assert_eq!((corrected(Some(true)).autocorrect, corrected(Some(false)).autocorrect, corrected(None).autocorrect), ("on", "off", ""));
    assert_eq!((corrected(Some(false)).spellcheck, corrected(None).spellcheck), ("false", ""));
    assert_eq!(Tokens::of(&Asked::default().resolve(false, true)).spellcheck, "true");
    use EnterKeyHint::*;
    for (hint, name) in [(Enter, ""), (Done, "done"), (Go, "go"), (Next, "next"), (Search, "search"), (Send, "send")] {
        assert_eq!(tokens(Asked { enter_key_hint: hint, ..Asked::default() }).enter_key_hint, name);
    }
}

#[test]
fn a_secure_field_is_a_password_for_the_browser_to_fill() {
    let secure = |asked: Asked| Tokens::of(&asked.resolve(true, false));
    let password = secure(Asked::default());
    assert_eq!((password.autocomplete, password.input_mode, password.autocapitalize), ("current-password", "text", "none"));
    assert_eq!((password.autocorrect, password.spellcheck), ("off", "false"));
    assert_eq!(secure(Asked { autocomplete: Autocomplete::NewPassword, ..Asked::default() }).autocomplete, "new-password");
    assert_eq!(secure(Asked { mode: Some(InputMode::Numeric), ..Asked::default() }).input_mode, "numeric");
}
