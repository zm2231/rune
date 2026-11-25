use super::*;

const PHONE: (f64, f64) = (402.0, 874.0);

#[test]
fn a_docked_keyboard_covers_the_band_below_its_top() {
    assert_eq!(inset(PHONE, [0.0, 538.0, 402.0, 336.0]), 336.0);
}

#[test]
fn a_keyboard_off_the_bottom_or_narrow_covers_nothing() {
    assert_eq!(inset(PHONE, [0.0, 874.0, 402.0, 336.0]), 0.0, "below the screen as it hides");
    assert_eq!(inset(PHONE, [100.0, 300.0, 320.0, 260.0]), 0.0, "floating mid-screen");
    assert_eq!(inset((1024.0, 1366.0), [700.0, 1000.0, 320.0, 366.0]), 0.0, "a floating iPad keyboard at the corner");
}

#[test]
fn a_view_shorter_than_the_screen_counts_only_its_own_overlap() {
    // A view whose bottom sits 100 points above the screen's, the keyboard in its points.
    assert_eq!(inset((402.0, 774.0), [0.0, 438.0, 402.0, 336.0]), 336.0);
    assert_eq!(inset((402.0, 300.0), [0.0, -100.0, 402.0, 400.0]), 300.0, "never more than the view");
}

#[test]
fn the_inset_follows_the_keyboard_and_drops_when_it_hides() {
    let mut k = Keyboard::default();
    let run = |k: &mut Keyboard, e| step(k, e).into_iter().collect::<Vec<_>>();
    assert_eq!(run(&mut k, KeyboardEvent::WillChange { inset: 336.0 }), [KeyboardEffect::SetInset(336.0)]);
    assert_eq!(run(&mut k, KeyboardEvent::WillChange { inset: 336.0 }), [], "the same frame again");
    assert_eq!(run(&mut k, KeyboardEvent::WillChange { inset: 380.0 }), [KeyboardEffect::SetInset(380.0)], "the predictive bar grew");
    assert_eq!(run(&mut k, KeyboardEvent::WillChange { inset: 0.0 }), [KeyboardEffect::SetInset(0.0)]);
    assert_eq!(k, Keyboard::Floating);
    assert_eq!(run(&mut k, KeyboardEvent::WillHide), []);
    assert_eq!(k, Keyboard::Hidden);
}
