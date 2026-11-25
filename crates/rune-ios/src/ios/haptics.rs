use std::cell::OnceCell;

use objc2::rc::Retained;
use objc2_ui_kit::{
    UIFeedbackGenerator, UIImpactFeedbackGenerator, UIImpactFeedbackStyle, UINotificationFeedbackGenerator,
    UINotificationFeedbackType, UISelectionFeedbackGenerator, UIView,
};
use rune::Haptic;

use crate::bridge::{Feedback, feedback};

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
        match feedback(haptic) {
            Feedback::Selection => self.selection(view).selectionChanged(),
            Feedback::Impact(style) => {
                let Some(slot) = usize::try_from(style).ok().and_then(|i| self.impact.get(i)) else { return };
                slot.get_or_init(|| UIImpactFeedbackGenerator::feedbackGeneratorWithStyle_forView(UIImpactFeedbackStyle(style), view))
                    .impactOccurred();
            }
            Feedback::Notification(kind) => {
                self.notification
                    .get_or_init(|| UINotificationFeedbackGenerator::feedbackGeneratorForView(view))
                    .notificationOccurred(UINotificationFeedbackType(kind));
            }
        }
    }

    /// Wakes the Taptic Engine when a finger lands, so a gesture's first haptic isn't late.
    pub(crate) fn prepare(&self, view: &UIView) {
        AsRef::<UIFeedbackGenerator>::as_ref(&**self.selection(view)).prepare();
    }

    fn selection(&self, view: &UIView) -> &Retained<UISelectionFeedbackGenerator> {
        self.selection.get_or_init(|| UISelectionFeedbackGenerator::feedbackGeneratorForView(view))
    }
}
