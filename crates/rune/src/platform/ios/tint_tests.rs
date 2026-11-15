use super::*;

fn rgb(r: u8, g: u8, b: u8) -> Option<Components> {
    Some([r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0, 1.0])
}

#[test]
fn the_default_accents_blue_in_srgb_is_the_default_accent() {
    // `#007AFF` and `#0A84FF` as UIKit hands a colour over.
    let blue = accent(rgb(0, 122, 255), rgb(10, 132, 255));
    let default = SystemAccent::DEFAULT;
    for dark in [false, true] {
        assert_eq!(blue.accent(dark).hex(), default.accent(dark).hex(), "dark {dark}");
        assert_eq!(blue.focus(dark).hex(), default.focus(dark).hex(), "dark {dark}");
    }
}

#[test]
fn a_tint_is_decoded_from_srgb_for_each_appearance() {
    // `systemPink` in light and in dark.
    let pink = accent(rgb(255, 45, 85), rgb(255, 55, 95));
    assert_eq!((pink.accent(false).hex(), pink.accent(true).hex()), ("#ff2d55ff".into(), "#ff375fff".into()));
    let light = pink.accent(false);
    assert_eq!(light.r, 1.0);
    assert!((light.g - crate::color::srgb_decode(45.0 / 255.0)).abs() < 1e-6, "linear, not the encoded {}", 45.0 / 255.0);
}

#[test]
fn focus_rings_are_the_tint_at_half_its_alpha() {
    let pink = accent(rgb(255, 45, 85), rgb(255, 55, 95));
    assert_eq!((pink.focus(false).hex(), pink.focus(true).hex()), ("#ff2d5580".into(), "#ff375f80".into()));
    let faint = accent(Some([1.0, 0.0, 0.0, 0.5]), Some([1.0, 0.0, 0.0, 0.5]));
    assert_eq!((faint.accent(false).a, faint.focus(false).a), (0.5, 0.25));
}

#[test]
fn a_colour_outside_srgb_is_clipped_to_it() {
    // Display P3's pure red and pure green in extended sRGB.
    let red = color(Some([1.093, -0.227, -0.150, 1.0])).unwrap();
    assert_eq!(red, Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 });
    let green = color(Some([-0.512, 1.018, -0.311, 1.0])).unwrap();
    assert_eq!(green, Color { r: 0.0, g: 1.0, b: 0.0, a: 1.0 });
    // An extended-range white brighter than the display's.
    assert_eq!(color(Some([2.0, 2.0, 2.0, 1.5])).unwrap(), Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 });
}

#[test]
fn a_clear_tint_stays_clear() {
    assert_eq!(color(Some([0.0, 0.0, 0.0, 0.0])).unwrap().a, 0.0);
    assert_eq!(color(Some([0.2, 0.4, 0.6, -0.5])).unwrap().a, 0.0);
}

#[test]
fn a_tint_with_no_colour_is_the_default_accent() {
    assert_eq!(color(None), None);
    assert_eq!(accent(None, None), SystemAccent::DEFAULT);
    // One appearance alone is no accent: both are needed.
    assert_eq!(accent(rgb(255, 45, 85), None), SystemAccent::DEFAULT);
    assert_eq!(accent(None, rgb(255, 55, 95)), SystemAccent::DEFAULT);
}

#[test]
fn a_component_that_is_no_number_is_no_colour() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for at in 0..4 {
            let mut components = [0.5; 4];
            components[at] = bad;
            assert_eq!(color(Some(components)), None, "{bad} at {at}");
            assert_eq!(accent(Some(components), rgb(1, 2, 3)), SystemAccent::DEFAULT);
        }
    }
}
