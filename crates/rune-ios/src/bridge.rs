//! How UIKit's values become Rune's and back, kept free of UIKit so tests run on any host.

use rune::{Haptic, HapticOutcome, ImpactStrength};

/// `UIEdgeInsets` (top, left, bottom, right) as `Runtime::set_safe_area` takes them: top,
/// right, bottom, left.
pub(crate) fn safe_area(top: f64, left: f64, bottom: f64, right: f64) -> [f32; 4] {
    [top as f32, right as f32, bottom as f32, left as f32]
}

/// The feedback generator that plays a haptic, with its `UIImpactFeedbackStyle` or
/// `UINotificationFeedbackType` raw value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Feedback {
    Selection,
    Impact(isize),
    Notification(isize),
}

pub(crate) fn feedback(haptic: Haptic) -> Feedback {
    match haptic {
        Haptic::Selection => Feedback::Selection,
        Haptic::Impact(strength) => Feedback::Impact(match strength {
            ImpactStrength::Light => 0,
            ImpactStrength::Medium => 1,
            ImpactStrength::Heavy => 2,
            ImpactStrength::Soft => 3,
            ImpactStrength::Rigid => 4,
        }),
        Haptic::Notify(outcome) => Feedback::Notification(match outcome {
            HapticOutcome::Success => 0,
            HapticOutcome::Warning => 1,
            HapticOutcome::Error => 2,
        }),
    }
}

#[cfg(test)]
#[path = "bridge_tests.rs"]
mod tests;
