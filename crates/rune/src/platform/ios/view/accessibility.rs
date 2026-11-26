//! A `RuneView` between its accessibility elements and its surface: a client's first ask for
//! the elements starts the runtime's tree, and what a reader does to an element goes to the
//! runtime as the tree's action.

use objc2::DefinedClass;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;

use super::super::pacing::Event;
use super::super::text::input::RuneTextInput;
use super::RuneView;
use crate::a11y::ios::Gesture;

impl RuneView {
    /// The view's elements, for a client that asks. The first ask starts the runtime's tree,
    /// and the elements follow the frame it wakes.
    pub(super) fn accessibility_asked(&self) -> Retained<AnyObject> {
        let (list, first) = self.ivars().accessibility.borrow_mut().elements();
        if first {
            self.with_surface(|surface| surface.accessibility_wanted());
            self.pace(Event::Wake);
        }
        list
    }

    /// The text input, while the keyboard edits a field through it that is not a password.
    pub(crate) fn readable_input(&self) -> Option<Retained<RuneTextInput>> {
        let input = self.ivars().text.borrow().clone()?;
        input.readable().then_some(input)
    }

    /// What a reader did to element `id`: true when the element takes it, which the next frame
    /// then does.
    pub(crate) fn accessibility_gesture(&self, id: u64, gesture: Gesture) -> bool {
        let Some(request) = self.ivars().accessibility.borrow_mut().request(id, gesture) else { return false };
        self.with_surface(|surface| surface.accessibility_act(request));
        self.pace(Event::Wake);
        true
    }
}
