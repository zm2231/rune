use super::*;

/// Ctrl goes down, then `shift`, then `shift` comes up: what the release sets.
fn chord(shift: Modifier) -> Option<Flow> {
    let (state, _) = step_dir_chord(DirChord::Idle, Modifier::ControlLeft, true, true, false);
    let (state, _) = step_dir_chord(state, shift, true, true, true);
    step_dir_chord(state, shift, false, true, false).1
}

/// Where ⌘ is the primary modifier (macOS, iOS) Ctrl and a Shift set no direction; elsewhere
/// the left Shift sets left to right and the right one right to left.
#[test]
fn ctrl_and_a_shift_set_the_direction_only_where_ctrl_is_the_primary_modifier() {
    let set = |flow| (!crate::MAC_KEYS).then_some(flow);
    assert_eq!(chord(Modifier::ShiftLeft), set(Flow::Ltr));
    assert_eq!(chord(Modifier::ShiftRight), set(Flow::Rtl));
}

/// Another modifier between Ctrl and the Shift's release disarms the chord.
#[test]
fn a_key_between_disarms_the_chord() {
    let (state, _) = step_dir_chord(DirChord::Idle, Modifier::ControlLeft, true, true, false);
    let (state, _) = step_dir_chord(state, Modifier::ShiftLeft, true, true, true);
    let (state, set) = step_dir_chord(state, Modifier::ShiftRight, true, true, true);
    assert_eq!((state, set), (DirChord::Idle, None));
    assert_eq!(step_dir_chord(state, Modifier::ShiftLeft, false, true, true), (DirChord::Idle, None));
}
