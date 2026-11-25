//! Whether the view's text input is first responder, and what UIKit last saw of the focused
//! field, so a change Rune or the app made on its own reaches UIKit's input delegate and one
//! UIKit made itself never echoes back (an echo resets autocorrect and the candidate bar).
//!
//! UIKit must not be called back from inside one of its own text input calls, which run a frame
//! synchronously: while a call is open the effects wait, and once it ends the view asks for a
//! main-queue turn ([`EditingEffect::ScheduleFlush`]) and runs them in [`EditingEvent::Flush`].

use std::ops::Range;

use rune::machine::Effects;
#[cfg(target_os = "ios")]
use rune::HostText;
use rune::TextHints;

/// What UIKit sees of the focused field.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Field {
    pub(crate) key: String,
    pub(crate) text: String,
    pub(crate) selection: (usize, usize),
    pub(crate) marked: Option<Range<usize>>,
    pub(crate) traits: Traits,
    /// Where the field's text sits in the text input view, which is framed on the part shown.
    pub(crate) offset: [f32; 2],
}

/// What the keyboard is configured from: a change reloads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Traits {
    pub(crate) hints: TextHints,
    pub(crate) secure: bool,
    pub(crate) single_line: bool,
}

impl Field {
    #[cfg(target_os = "ios")]
    pub(crate) fn of(t: &HostText) -> Field {
        Field {
            key: t.key.clone(),
            text: t.text.clone(),
            selection: (t.anchor, t.focus),
            marked: t.marked.clone(),
            traits: Traits { hints: t.hints, secure: t.secure, single_line: t.single_line },
            offset: [t.rect[0] - t.visible[0], t.rect[1] - t.visible[1]],
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum Editing {
    /// No editable field has focus; the text input view isn't first responder.
    #[default]
    Idle,
    /// The text input view is first responder for `field`, as UIKit last saw it.
    Active { field: Field },
}

/// Whether one of UIKit's text input calls is open, and the effects waiting for it to end.
#[derive(Clone, Debug, Default, PartialEq)]
enum Call {
    #[default]
    Outside,
    Inside { depth: u32, held: Vec<EditingEffect> },
    /// The call ended; `held` runs on the main-queue turn the view asked for.
    Pending { held: Vec<EditingEffect> },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Editor {
    pub(crate) state: Editing,
    call: Call,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum EditingEvent {
    /// A frame Rune ran on its own: the focused editable field now, if any.
    Framed(Option<Field>),
    /// UIKit changed the field itself (and a frame ran): the field now.
    Edited(Option<Field>),
    CallBegan,
    CallEnded,
    Flush,
    /// UIKit took first responder away (the keyboard was dismissed).
    Resigned,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum EditingEffect {
    BecomeFirstResponder,
    ResignFirstResponder,
    /// The keyboard's traits changed: `reloadInputViews`.
    ReloadInputViews,
    /// The text changed behind UIKit's back: `textWillChange:`/`textDidChange:`.
    TextChanged,
    /// The selection, or where it draws, changed behind UIKit's back.
    SelectionChanged,
    /// The person dismissed the keyboard: blur the field `key`.
    Blur(String),
    /// Feed [`EditingEvent::Flush`] on a later main-queue turn.
    ScheduleFlush,
}

pub(crate) fn step(editor: &mut Editor, event: EditingEvent) -> Effects<EditingEffect> {
    match event {
        EditingEvent::CallBegan => {
            editor.call = match std::mem::take(&mut editor.call) {
                Call::Outside => Call::Inside { depth: 1, held: Vec::new() },
                Call::Inside { depth, held } => Call::Inside { depth: depth + 1, held },
                Call::Pending { held } => Call::Inside { depth: 1, held },
            };
            Effects::none()
        }
        EditingEvent::CallEnded => match std::mem::take(&mut editor.call) {
            Call::Inside { depth, held } if depth > 1 => {
                editor.call = Call::Inside { depth: depth - 1, held };
                Effects::none()
            }
            Call::Inside { held, .. } if held.is_empty() => Effects::none(),
            Call::Inside { held, .. } => {
                editor.call = Call::Pending { held };
                Effects::one(EditingEffect::ScheduleFlush)
            }
            other => {
                editor.call = other;
                Effects::none()
            }
        },
        EditingEvent::Flush => match std::mem::take(&mut editor.call) {
            Call::Pending { held } => all(held),
            other => {
                editor.call = other;
                Effects::none()
            }
        },
        EditingEvent::Framed(now) => {
            let effects = transition(&mut editor.state, now, false);
            release(editor, effects)
        }
        EditingEvent::Edited(now) => {
            let effects = transition(&mut editor.state, now, true);
            release(editor, effects)
        }
        EditingEvent::Resigned => {
            let effects = match std::mem::take(&mut editor.state) {
                Editing::Active { field } => vec![EditingEffect::Blur(field.key)],
                Editing::Idle => Vec::new(),
            };
            release(editor, effects)
        }
    }
}

/// Emits `effects` now outside a call, or holds them for its end.
fn release(editor: &mut Editor, effects: Vec<EditingEffect>) -> Effects<EditingEffect> {
    match &mut editor.call {
        Call::Outside => all(effects),
        Call::Inside { held, .. } | Call::Pending { held } => {
            for effect in effects {
                if !held.contains(&effect) { held.push(effect); }
            }
            Effects::none()
        }
    }
}

fn all(effects: Vec<EditingEffect>) -> Effects<EditingEffect> {
    let mut out = Effects::none();
    out.extend(effects);
    out
}

/// The field is now `now`; `uikit` when UIKit made the change itself and already knows it.
fn transition(state: &mut Editing, now: Option<Field>, uikit: bool) -> Vec<EditingEffect> {
    let before = std::mem::take(state);
    let Some(field) = now else {
        return match before {
            Editing::Active { .. } => vec![EditingEffect::ResignFirstResponder],
            Editing::Idle => Vec::new(),
        };
    };
    let mut effects = Vec::new();
    match &before {
        Editing::Idle => effects.push(EditingEffect::BecomeFirstResponder),
        Editing::Active { field: seen } => {
            if seen.traits != field.traits {
                effects.push(EditingEffect::ReloadInputViews);
            }
            if seen.key != field.key {
                effects.push(EditingEffect::TextChanged);
            } else if !uikit {
                if seen.text != field.text || seen.marked != field.marked {
                    effects.push(EditingEffect::TextChanged);
                } else if seen.selection != field.selection || seen.offset != field.offset {
                    effects.push(EditingEffect::SelectionChanged);
                }
            } else if seen.offset != field.offset {
                effects.push(EditingEffect::SelectionChanged);
            }
        }
    }
    *state = Editing::Active { field };
    effects
}

#[cfg(test)]
#[path = "editing_tests.rs"]
mod tests;
