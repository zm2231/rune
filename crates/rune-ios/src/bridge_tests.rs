use rune::{Cx, El, Env, Haptic, HapticOutcome, ImpactStrength, Runtime, col, el};

use super::{Feedback, feedback, safe_area};

fn inset_view(_: &(), cx: &mut Cx<()>) -> El {
    let [top, right, bottom, left] = cx.safe_area();
    col().w_full().h_full().pt(top).pr(right).pb(bottom).pl(left).child(el().key("content").w_full().h_full())
}

#[test]
fn uikit_insets_reach_the_view_on_their_own_edges() {
    let mut rt = Runtime::new((), inset_view, (874.0, 402.0), Env { system_dark: false, scale: 3.0 });
    // An iPhone in landscape with the Dynamic Island on the left; every edge differs so a swap shows.
    rt.set_safe_area(safe_area(4.0, 62.0, 21.0, 11.0));
    rt.frame(0.0);
    let [x, y, w, h] = rt.presented("content").unwrap_or_else(|| panic!("no content")).0;
    assert_eq!([x, y], [62.0, 4.0]);
    assert_eq!([w, h], [874.0 - 62.0 - 11.0, 402.0 - 4.0 - 21.0]);
}

#[test]
fn haptics_pick_uikit_generators_and_raw_styles() {
    let cases = [
        (Haptic::Selection, Feedback::Selection),
        (Haptic::Impact(ImpactStrength::Light), Feedback::Impact(0)),
        (Haptic::Impact(ImpactStrength::Medium), Feedback::Impact(1)),
        (Haptic::Impact(ImpactStrength::Heavy), Feedback::Impact(2)),
        (Haptic::Impact(ImpactStrength::Soft), Feedback::Impact(3)),
        (Haptic::Impact(ImpactStrength::Rigid), Feedback::Impact(4)),
        (Haptic::Notify(HapticOutcome::Success), Feedback::Notification(0)),
        (Haptic::Notify(HapticOutcome::Warning), Feedback::Notification(1)),
        (Haptic::Notify(HapticOutcome::Error), Feedback::Notification(2)),
    ];
    for (haptic, expected) in cases {
        assert_eq!(feedback(haptic), expected, "{haptic:?}");
    }
}
