use std::cell::OnceCell;

use objc2::rc::Retained;
use objc2_ui_kit::{
    UIFeedbackGenerator, UIImpactFeedbackGenerator, UIImpactFeedbackStyle, UINotificationFeedbackGenerator,
    UINotificationFeedbackType, UISelectionFeedbackGenerator, UIView,
};
use rune::{Haptic, HapticOutcome, ImpactStrength};

/// A view's feedback generators, made on first use and attached to the view so the system
/// plays them on its screen.
#[derive(Default)]
pub(crate) struct Haptics {
    selection: OnceCell<Retained<UISelectionFeedbackGenerator>>,
    impact: [OnceCell<Retained<UIImpactFeedbackGenerator>>; 5],
    notification: OnceCell<Retained<UINotificationFeedbackGenerator>>,
}

impl Haptics {
    pub(crate) fn play(&self, view: &UIView, haptic: Haptic) {
        match haptic {
            Haptic::Selection => {
                self.selection.get_or_init(|| UISelectionFeedbackGenerator::feedbackGeneratorForView(view)).selectionChanged();
            }
            Haptic::Impact(strength) => {
                let (slot, style) = match strength {
                    ImpactStrength::Light => (0, UIImpactFeedbackStyle::Light),
                    ImpactStrength::Medium => (1, UIImpactFeedbackStyle::Medium),
                    ImpactStrength::Heavy => (2, UIImpactFeedbackStyle::Heavy),
                    ImpactStrength::Soft => (3, UIImpactFeedbackStyle::Soft),
                    ImpactStrength::Rigid => (4, UIImpactFeedbackStyle::Rigid),
                };
                self.impact[slot]
                    .get_or_init(|| UIImpactFeedbackGenerator::feedbackGeneratorWithStyle_forView(style, view))
                    .impactOccurred();
            }
            Haptic::Notify(outcome) => {
                let kind = match outcome {
                    HapticOutcome::Success => UINotificationFeedbackType::Success,
                    HapticOutcome::Warning => UINotificationFeedbackType::Warning,
                    HapticOutcome::Error => UINotificationFeedbackType::Error,
                };
                self.notification
                    .get_or_init(|| UINotificationFeedbackGenerator::feedbackGeneratorForView(view))
                    .notificationOccurred(kind);
            }
        }
    }

    /// Wakes the Taptic Engine when a finger lands, so a gesture's first haptic isn't late.
    pub(crate) fn prepare(&self, view: &UIView) {
        let selection = self.selection.get_or_init(|| UISelectionFeedbackGenerator::feedbackGeneratorForView(view));
        AsRef::<UIFeedbackGenerator>::as_ref(&**selection).prepare();
    }
}
