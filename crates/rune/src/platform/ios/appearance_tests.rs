use super::Appearance;
use crate::ColorScheme;

#[test]
fn a_view_overrides_its_style_only_when_the_effective_scheme_changes() {
    let mut a = Appearance::new(ColorScheme::System);
    assert_eq!(a.changed(), None);
    a.set_app(ColorScheme::Dark);
    assert_eq!(a.changed(), Some(2));
    assert_eq!(a.changed(), None);
    a.set_window(ColorScheme::Light);
    assert_eq!(a.changed(), Some(1));
    a.set_window(ColorScheme::System);
    assert_eq!(a.changed(), Some(2));
    a.set_app(ColorScheme::System);
    assert_eq!(a.changed(), Some(0));
    assert_eq!(Appearance::new(ColorScheme::Dark).changed(), Some(2));
}
