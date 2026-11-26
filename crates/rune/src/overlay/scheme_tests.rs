use super::*;
use crate::ColorScheme;

fn overlay() -> Overlay<()> {
    fn view(_: &(), _: &mut Cx<()>) -> El { crate::el() }
    let app = AppSpec::new("scheme", (200.0, 100.0), || (), view);
    let (overlay, _) = Overlay::new(OverlaySpec::new("scheme", app.size, view).options(Default::default()), (), &app);
    overlay
}

fn app_scheme(overlay: &mut Overlay<()>, scheme: ColorScheme) {
    overlay.core.app_scheme_changed(overlay.rt.core_mut(), scheme);
}

#[test]
fn an_overlay_follows_the_app_scheme_and_returns_to_the_system_one() {
    let mut overlay = overlay();
    overlay.core.theme_changed(overlay.rt.core_mut(), false);
    assert!(!overlay.rt.env.system_dark);

    overlay.core.dirty = false;
    app_scheme(&mut overlay, ColorScheme::Dark);
    assert!(overlay.rt.env.system_dark, "an app forcing dark on a light system draws its overlays dark");
    assert!(overlay.dirty, "a hidden overlay redraws in the app's scheme before it's revealed");

    overlay.core.theme_changed(overlay.rt.core_mut(), false);
    assert!(overlay.rt.env.system_dark, "the system's light appearance doesn't undo the app's dark");

    app_scheme(&mut overlay, ColorScheme::System);
    assert!(!overlay.rt.env.system_dark, "back to System, the overlay takes the system's light appearance");
}

#[test]
fn an_overlays_own_scheme_wins_over_the_app_scheme_until_it_returns_to_system() {
    let mut overlay = overlay();
    overlay.core.theme_changed(overlay.rt.core_mut(), false);
    app_scheme(&mut overlay, ColorScheme::Dark);

    overlay.rt.host.window_color_scheme = Some(ColorScheme::Light);
    overlay.take_window_requests();
    assert!(!overlay.rt.env.system_dark, "Tx::set_color_scheme(Light) in the overlay's handler beats the app's dark");

    overlay.rt.host.window_color_scheme = Some(ColorScheme::System);
    overlay.take_window_requests();
    assert!(overlay.rt.env.system_dark, "the overlay inherits the app's dark again");
}
