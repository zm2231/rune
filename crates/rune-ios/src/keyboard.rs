//! How much of the view the software keyboard covers. UIKit says where the keyboard will end
//! up as it moves; the inset is the view's height below the keyboard's top while the keyboard is
//! docked across the view's bottom. A floating, undocked or split keyboard covers no band, so it
//! insets nothing. The view's own springs move what reads the inset, so the keyboard machine
//! only says where it lands.

use rune::machine::Effects;

/// A rect `[x, y, w, h]` in the view's points.
pub(crate) type Rect = [f64; 4];

/// The keyboard covers the bottom of a view of size `view` when it reaches the view's bottom
/// and spans most of its width; it covers the band from its top down.
pub(crate) fn inset(view: (f64, f64), keyboard: Rect) -> f32 {
    let [x, y, w, h] = keyboard;
    let (vw, vh) = view;
    let across = (x + w).min(vw) - x.max(0.0);
    let docked = y + h >= vh - 0.5 && across >= vw * 0.5 && h > 0.0;
    if !docked { return 0.0; }
    (vh - y).clamp(0.0, vh) as f32
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum Keyboard {
    #[default]
    Hidden,
    /// Docked, covering `inset` points of the view's bottom.
    Docked { inset: f32 },
    /// Up, but floating or split: it covers no band.
    Floating,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum KeyboardEvent {
    /// UIKit's `keyboardWillChangeFrame`: where it lands, as an inset of the view.
    WillChange { inset: f32 },
    WillHide,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum KeyboardEffect {
    SetInset(f32),
}

pub(crate) fn step(state: &mut Keyboard, event: KeyboardEvent) -> Effects<KeyboardEffect> {
    let before = covered(*state);
    *state = match event {
        KeyboardEvent::WillChange { inset } if inset > 0.0 => Keyboard::Docked { inset },
        KeyboardEvent::WillChange { .. } => Keyboard::Floating,
        KeyboardEvent::WillHide => Keyboard::Hidden,
    };
    let after = covered(*state);
    if after == before { Effects::none() } else { Effects::one(KeyboardEffect::SetInset(after)) }
}

fn covered(state: Keyboard) -> f32 {
    match state {
        Keyboard::Docked { inset } => inset,
        Keyboard::Hidden | Keyboard::Floating => 0.0,
    }
}

#[cfg(test)]
#[path = "keyboard_tests.rs"]
mod tests;
