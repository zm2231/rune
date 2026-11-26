use super::*;
use crate::platform::ios::keys;

use Effect::{Focus, Quit, Rest, Visibility};

fn run(state: &mut State, event: Event) -> Vec<Effect> {
    step(state, event).into_iter().collect()
}

/// A view in a window of a scene in this phase.
fn attached(phase: Phase) -> State {
    let mut state = State::default();
    run(&mut state, Event::Attached(phase));
    state
}

const STATES: [State; 5] = [State::Detached, State::Background, State::Inactive, State::Active, State::Terminated];
const EVENTS: [Event; 9] = [
    Event::Attached(Phase::Background),
    Event::Attached(Phase::Inactive),
    Event::Attached(Phase::Active),
    Event::Detached,
    Event::Foreground,
    Event::Background,
    Event::Active,
    Event::Inactive,
    Event::Terminate,
];

#[test]
fn a_scene_state_names_its_phase_and_a_window_with_no_scene_asks_the_app() {
    assert_eq!(Phase::from_raw(Some(0), 2), Phase::Active);
    assert_eq!(Phase::from_raw(Some(1), 0), Phase::Inactive);
    assert_eq!(Phase::from_raw(Some(2), 0), Phase::Background);
    // A scene still connecting is in the background, whatever the app says.
    assert_eq!(Phase::from_raw(Some(-1), 0), Phase::Background);
    assert_eq!(Phase::from_raw(Some(-1), 1), Phase::Background);
    assert_eq!(Phase::from_raw(None, 0), Phase::Active);
    assert_eq!(Phase::from_raw(None, 1), Phase::Inactive);
    assert_eq!(Phase::from_raw(None, 2), Phase::Background);
}

#[test]
fn a_view_that_joins_an_active_scene_is_visible_then_focused() {
    let mut state = State::default();
    assert_eq!(run(&mut state, Event::Attached(Phase::Active)), [Visibility(true), Focus(true)]);
    assert_eq!(state, State::Active);
}

#[test]
fn a_view_that_joins_a_scene_coming_forward_is_visible_and_focused_once_it_is_active() {
    let mut state = State::default();
    assert_eq!(run(&mut state, Event::Attached(Phase::Inactive)), [Visibility(true)]);
    // The scene's own notifications: the first repeats what the view read.
    assert_eq!(run(&mut state, Event::Foreground), []);
    assert_eq!(run(&mut state, Event::Active), [Focus(true)]);
    assert_eq!(state, State::Active);
}

/// A launch: the view joins its window while the scene connects, then the scene comes to the
/// foreground and becomes active.
#[test]
fn a_launch_shows_the_app_when_its_scene_comes_forward_and_focuses_it_when_active() {
    let mut state = State::default();
    assert_eq!(run(&mut state, Event::Attached(Phase::from_raw(Some(-1), 1))), []);
    assert_eq!(state, State::Background);
    assert_eq!(run(&mut state, Event::Foreground), [Visibility(true)]);
    assert_eq!(run(&mut state, Event::Active), [Focus(true)]);
    assert_eq!(state, State::Active);
}

/// A launch whose connecting scene posts no will-enter-foreground: the first the view hears
/// is that the scene is active.
#[test]
fn a_launch_with_no_foreground_notice_shows_then_focuses_the_app_when_its_scene_is_active() {
    let mut state = State::default();
    assert_eq!(run(&mut state, Event::Attached(Phase::from_raw(Some(-1), 1))), []);
    assert_eq!(state, State::Background);
    assert_eq!(run(&mut state, Event::Active), [Visibility(true), Focus(true)]);
    assert_eq!(state, State::Active);
}

#[test]
fn a_view_that_joins_a_scene_in_the_background_hears_nothing_until_it_comes_forward() {
    let mut state = State::default();
    assert_eq!(run(&mut state, Event::Attached(Phase::Background)), []);
    assert_eq!(run(&mut state, Event::Background), []);
    assert_eq!(run(&mut state, Event::Foreground), [Visibility(true)]);
    assert_eq!(run(&mut state, Event::Active), [Focus(true)]);
}

#[test]
fn going_away_loses_the_focus_before_it_hides() {
    let mut state = attached(Phase::Active);
    assert_eq!(run(&mut state, Event::Inactive), [Focus(false)]);
    assert_eq!(state, State::Inactive);
    assert_eq!(run(&mut state, Event::Background), [Visibility(false), Rest]);
    assert_eq!(state, State::Background);
}

#[test]
fn coming_back_shows_before_it_focuses() {
    let mut state = attached(Phase::Background);
    assert_eq!(run(&mut state, Event::Foreground), [Visibility(true)]);
    assert_eq!(run(&mut state, Event::Active), [Focus(true)]);
}

#[test]
fn control_center_takes_the_focus_and_gives_it_back_without_hiding() {
    let mut state = attached(Phase::Active);
    assert_eq!(run(&mut state, Event::Inactive), [Focus(false)]);
    assert_eq!(run(&mut state, Event::Active), [Focus(true)]);
    assert_eq!(state, State::Active);
}

#[test]
fn a_repeated_notification_changes_nothing() {
    for (phase, repeat) in [(Phase::Active, Event::Active), (Phase::Active, Event::Foreground), (Phase::Inactive, Event::Inactive), (Phase::Inactive, Event::Foreground), (Phase::Background, Event::Background), (Phase::Background, Event::Inactive)] {
        let mut state = attached(phase);
        let before = state;
        assert_eq!(run(&mut state, repeat), [], "{phase:?} {repeat:?}");
        assert_eq!(state, before);
    }
    for phase in [Phase::Background, Phase::Inactive, Phase::Active] {
        let mut state = attached(phase);
        assert_eq!(run(&mut state, Event::Attached(phase)), [], "{phase:?}");
    }
}

#[test]
fn a_background_notice_with_no_resign_before_it_brings_the_lost_focus_with_it() {
    let mut state = attached(Phase::Active);
    assert_eq!(run(&mut state, Event::Background), [Focus(false), Visibility(false), Rest]);
    assert_eq!(state, State::Background);
    // The late resign changes nothing.
    assert_eq!(run(&mut state, Event::Inactive), []);
    assert_eq!(state, State::Background);
}

#[test]
fn an_active_notice_with_no_foreground_before_it_shows_the_app_first() {
    let mut state = attached(Phase::Background);
    assert_eq!(run(&mut state, Event::Active), [Visibility(true), Focus(true)]);
    // The late foreground changes nothing.
    assert_eq!(run(&mut state, Event::Foreground), []);
    assert_eq!(state, State::Active);
}

#[test]
fn a_view_that_leaves_its_window_is_hidden_and_one_that_returns_shows_again() {
    let mut state = attached(Phase::Active);
    assert_eq!(run(&mut state, Event::Detached), [Focus(false), Visibility(false), Rest]);
    assert_eq!(state, State::Detached);
    assert_eq!(run(&mut state, Event::Attached(Phase::Active)), [Visibility(true), Focus(true)]);

    let mut state = attached(Phase::Inactive);
    assert_eq!(run(&mut state, Event::Detached), [Visibility(false), Rest]);
    let mut state = attached(Phase::Background);
    assert_eq!(run(&mut state, Event::Detached), []);
}

#[test]
fn a_view_that_moves_to_another_scene_hears_where_that_one_is() {
    let mut state = attached(Phase::Active);
    assert_eq!(run(&mut state, Event::Attached(Phase::Inactive)), [Focus(false)]);
    assert_eq!(run(&mut state, Event::Attached(Phase::Background)), [Visibility(false), Rest]);
    assert_eq!(run(&mut state, Event::Attached(Phase::Active)), [Visibility(true), Focus(true)]);
}

#[test]
fn a_view_in_no_window_hears_no_scene() {
    for event in [Event::Foreground, Event::Background, Event::Active, Event::Inactive, Event::Detached] {
        let mut state = State::Detached;
        assert_eq!(run(&mut state, event), [], "{event:?}");
        assert_eq!(state, State::Detached);
    }
}

#[test]
fn the_focus_goes_once_for_each_time_the_scene_stops_being_active() {
    let mut state = attached(Phase::Active);
    let mut lost = 0;
    for event in [Event::Inactive, Event::Background, Event::Detached, Event::Attached(Phase::Background), Event::Foreground] {
        lost += run(&mut state, event).iter().filter(|effect| **effect == Focus(false)).count();
    }
    assert_eq!(lost, 1);
    // Active again, a key can be held again.
    run(&mut state, Event::Active);
    assert_eq!(run(&mut state, Event::Background), [Focus(false), Visibility(false), Rest]);
}

/// Losing the focus is what releases held input, so every way out of an active scene has it,
/// and has it first.
#[test]
fn every_way_out_of_an_active_scene_loses_the_focus_first() {
    for event in [Event::Inactive, Event::Background, Event::Detached, Event::Attached(Phase::Inactive), Event::Attached(Phase::Background), Event::Terminate] {
        let mut state = attached(Phase::Active);
        assert_eq!(run(&mut state, event).first(), Some(&Focus(false)), "{event:?}");
    }
}

/// Every way out of sight but the app's end: the view rests last, after the app heard it hide.
#[test]
fn a_view_that_hides_rests_after_its_app_heard_it_hide() {
    for from in [Phase::Active, Phase::Inactive] {
        for event in [Event::Background, Event::Detached, Event::Attached(Phase::Background)] {
            let mut state = attached(from);
            let effects = run(&mut state, event);
            assert_eq!(effects[effects.len() - 2..], [Visibility(false), Rest], "{from:?} {event:?}");
            assert_eq!(effects.iter().filter(|effect| **effect == Rest).count(), 1, "{from:?} {event:?}");
        }
    }
}

#[test]
fn a_view_already_out_of_sight_does_not_rest_again_until_it_has_been_in_front() {
    let mut state = attached(Phase::Active);
    assert_eq!(run(&mut state, Event::Background), [Focus(false), Visibility(false), Rest]);
    // Hidden already: leaving the window, joining another in the background, a late resign and
    // a repeated notice bring no second rest.
    for event in [Event::Background, Event::Inactive, Event::Detached, Event::Attached(Phase::Background), Event::Background] {
        assert_eq!(run(&mut state, event), [], "{event:?}");
    }
    assert_eq!(run(&mut state, Event::Foreground), [Visibility(true)]);
    assert_eq!(run(&mut state, Event::Background), [Visibility(false), Rest]);
}

/// Losing the focus alone hides nothing: Control Center and the app switcher leave the view
/// on screen, with everything it draws from.
#[test]
fn a_view_that_only_loses_the_focus_does_not_rest() {
    let mut state = attached(Phase::Active);
    assert_eq!(run(&mut state, Event::Inactive), [Focus(false)]);
    assert_eq!(run(&mut state, Event::Active), [Focus(true)]);
    assert_eq!(run(&mut state, Event::Attached(Phase::Inactive)), [Focus(false)]);
}

/// The app's memory goes with it, and what time it has left is `on_quit`'s.
#[test]
fn an_app_that_terminates_does_not_rest() {
    for from in STATES {
        let mut state = from;
        assert!(!run(&mut state, Event::Terminate).contains(&Rest), "{from:?}");
    }
}

#[test]
fn termination_takes_the_focus_and_hides_what_is_in_front_then_quits_once_and_ends_the_hooks() {
    let heard = [
        (State::Active, vec![Focus(false), Visibility(false), Quit]),
        (State::Inactive, vec![Visibility(false), Quit]),
        (State::Background, vec![Quit]),
        (State::Detached, vec![Quit]),
    ];
    for (from, heard) in heard {
        let mut state = from;
        assert_eq!(run(&mut state, Event::Terminate), heard, "{from:?}");
        assert_eq!(state, State::Terminated);
        for event in EVENTS {
            assert_eq!(run(&mut state, event), [], "{event:?}");
            assert_eq!(state, State::Terminated);
        }
    }
}

/// From every state, every event: a hook never hears the value it heard last, the focus is
/// never held while hidden, an app that quits has lost the focus and hidden, and a view rests
/// only just after it hid, so never twice without having been in front between, and never in
/// an app that is ending.
#[test]
fn no_hook_hears_a_value_twice_over_any_two_events() {
    for from in STATES {
        for first in EVENTS {
            for second in EVENTS {
                let mut state = from;
                let (mut visible, mut focused, mut quits) = (from.visible(), from.focused(), 0);
                // Whether the view has rested since it was last in front. One that starts
                // hidden was never shown, or rested when it hid.
                let mut rested = !from.visible();
                for event in [first, second] {
                    let effects = run(&mut state, event);
                    for (at, effect) in effects.iter().enumerate() {
                        match *effect {
                            Visibility(now) => {
                                assert_ne!(visible, now, "{from:?} {first:?} {second:?}");
                                visible = now;
                                if visible { rested = false; }
                            }
                            Rest => {
                                assert!(at > 0 && effects[at - 1] == Visibility(false), "{from:?} {first:?} {second:?}: a rest not just after hiding");
                                assert!(!rested, "{from:?} {first:?} {second:?}: rested twice");
                                assert_ne!(state, State::Terminated, "{from:?} {first:?} {second:?}: rested while ending");
                                rested = true;
                            }
                            Focus(now) => { assert_ne!(focused, now, "{from:?} {first:?} {second:?}"); focused = now; }
                            Quit => {
                                assert!(!visible && !focused && at + 1 == effects.len(), "{from:?} {first:?} {second:?}");
                                quits += 1;
                            }
                        }
                        assert!(visible || !focused, "{from:?} {first:?} {second:?}: focused while hidden");
                    }
                    if state != State::Terminated {
                        assert_eq!((visible, focused), (state.visible(), state.focused()));
                        // A view out of sight has rested.
                        assert_eq!(rested, !visible, "{from:?} {first:?} {second:?}");
                    }
                }
                assert!(quits <= 1);
            }
        }
    }
}

/// A grip under a finger that also hears the keys, and what it heard.
#[derive(Default)]
struct Held {
    heard: Vec<&'static str>,
}

fn grip(_: &Held, _: &mut crate::Cx<Held>) -> crate::El {
    crate::el().key("grip").size(100.0, 40.0).focusable(true)
        .on_drag(|s: &mut Held, drag: &mut crate::Drag, _| {
            if drag.phase == crate::Phase::Up { s.heard.push(if drag.cancelled() { "up, cancelled" } else { "up" }); }
        })
        .on_physical_input(|s: &mut Held, event, _| {
            match event {
                crate::PhysicalInput::Key { pressed: true, repeat, .. } => s.heard.push(if *repeat { "key, repeated" } else { "key" }),
                crate::PhysicalInput::FocusLost => s.heard.push("focus lost"),
                _ => {}
            }
            true
        })
}

/// A view's runtime and key repeat, driven as the view and its surface drive them.
struct Scene {
    rt: crate::Runtime<Held>,
    lifecycle: State,
    seq: crate::machine::Seq,
    repeat: keys::Repeat,
    /// `App::on_rest`, which the surface keeps for the view's rests.
    on_rest: Option<fn(&mut Held, &mut crate::Tx) -> bool>,
}

impl Scene {
    /// A lifecycle event and the frame after it: the key repeat is reset when the focus went.
    fn hears(&mut self, event: Event) {
        if self.told(event) { self.keys(keys::Event::Reset); }
        self.rt.frame(0.0);
    }

    /// A lifecycle event with no frame after it: the hooks' effects reach the runtime, with a
    /// rest's renderer left out. True when the focus went.
    fn told(&mut self, event: Event) -> bool {
        let mut released = false;
        for effect in step(&mut self.lifecycle, event) {
            match effect {
                Visibility(visible) => self.rt.set_live_visible(visible),
                Focus(focused) => {
                    self.rt.scene_focus(focused);
                    released |= !focused;
                }
                Rest => self.rt.rest_with(self.on_rest, None, true),
                Quit => {}
            }
        }
        released
    }

    /// A key event: the strokes it makes reach the runtime. The timer it asks for, if any.
    fn keys(&mut self, event: keys::Event) -> Option<crate::machine::Attempt> {
        let mut timer = None;
        for effect in keys::step(&mut self.seq, &mut self.repeat, event) {
            match effect {
                keys::Effect::Down { stroke, repeat } => {
                    let input = crate::PhysicalInput::Key { key: stroke.code.unwrap(), mods: stroke.mods, pressed: true, repeat, text: None, consumed_mods: Default::default() };
                    self.rt.physical_key_input(input, stroke.key.unwrap(), stroke.base);
                }
                keys::Effect::Up(_) => {}
                keys::Effect::TimerAfter(attempt, _) => timer = Some(attempt),
            }
        }
        self.rt.frame(0.0);
        timer
    }
}

/// A finger down on a drag and a hardware key held while the app is put away: the scene
/// resigns active and then goes to the background. The drag ends before the keys are
/// released, as in a window that loses the focus, each once, and the held key's repeat
/// timer sends no stroke after it.
#[test]
fn an_app_put_away_cancels_the_drag_then_releases_the_keys_and_the_held_key_stops_repeating() {
    use crate::{Key, KeyEvent, Mods, PhysicalKeyCode};
    let rt = crate::Runtime::new(Held::default(), grip, (100.0, 40.0), crate::Env::new(true, 1.0));
    let mut scene = Scene { rt, lifecycle: State::default(), seq: Default::default(), repeat: keys::Repeat::Idle, on_rest: None };
    scene.rt.frame(0.0);
    scene.hears(Event::Attached(Phase::Active));
    scene.rt.pointer_down(10.0, 10.0);
    scene.rt.frame(0.0);
    let left = keys::Stroke { code: Some(PhysicalKeyCode::ArrowLeft), mods: Mods::default(), key: Some(KeyEvent { key: Key::Left, mods: Mods::default() }), base: None };
    let timer = scene.keys(keys::Event::Down { usage: 0x50, stroke: left.clone() }).unwrap();
    let timer = scene.keys(keys::Event::Timer(timer)).unwrap();
    assert_eq!(scene.rt.state.heard, ["key", "key, repeated"]);

    scene.hears(Event::Inactive);
    assert_eq!(scene.rt.state.heard[2..], ["up, cancelled", "focus lost"]);
    assert_eq!(scene.repeat, keys::Repeat::Idle);
    scene.hears(Event::Background);
    assert_eq!(scene.keys(keys::Event::Timer(timer)), None);
    assert_eq!(scene.rt.state.heard.len(), 4, "nothing twice, and no stroke from the held key's timer");
}

/// An app put away and brought back twice: each time it hears that it is hidden and then
/// rests, once, and a notice that repeats while it is away rests nothing more. An app that
/// ends while away hears no rest.
#[test]
fn an_app_put_away_rests_once_after_it_heard_it_was_hidden_and_again_only_after_it_came_back() {
    fn visibility(s: &mut Held, visible: bool, _: &mut crate::Tx) { s.heard.push(if visible { "shown" } else { "hidden" }); }
    fn rest(s: &mut Held, _: &mut crate::Tx) -> bool {
        s.heard.push("rest");
        false
    }
    let mut rt = crate::Runtime::new(Held::default(), grip, (100.0, 40.0), crate::Env::new(true, 1.0));
    rt.hooks.on_visibility = Some(visibility);
    rt.set_live_visible(false);
    let mut scene = Scene { rt, lifecycle: State::default(), seq: Default::default(), repeat: keys::Repeat::Idle, on_rest: Some(rest) };
    scene.rt.frame(0.0);
    scene.hears(Event::Attached(Phase::Active));
    assert_eq!(scene.rt.state.heard, ["shown"]);

    scene.hears(Event::Inactive);
    scene.hears(Event::Background);
    assert_eq!(scene.rt.state.heard[1..], ["hidden", "rest"]);
    for event in [Event::Background, Event::Inactive, Event::Detached, Event::Attached(Phase::Background)] { scene.hears(event); }
    assert_eq!(scene.rt.state.heard.len(), 3, "one rest for one time away");

    scene.hears(Event::Foreground);
    scene.hears(Event::Active);
    scene.hears(Event::Background);
    assert_eq!(scene.rt.state.heard[3..], ["shown", "hidden", "rest"]);

    scene.hears(Event::Terminate);
    assert_eq!(scene.rt.state.heard.len(), 6, "an app that ends does not rest");
}

/// A rest's hook that says it changed what the view reads: the runtime's last frame is out of
/// date, for the first frame back in front to run the view again. One that says nothing
/// changed leaves the frame good.
#[test]
fn a_rest_whose_hook_changed_state_leaves_the_next_frame_to_run_the_view_again() {
    fn dropped(s: &mut Held, _: &mut crate::Tx) -> bool {
        s.heard.push("rest");
        true
    }
    fn kept(s: &mut Held, _: &mut crate::Tx) -> bool {
        s.heard.push("rest");
        false
    }
    for (hook, stale) in [(dropped as fn(&mut Held, &mut crate::Tx) -> bool, true), (kept, false)] {
        let rt = crate::Runtime::new(Held::default(), grip, (100.0, 40.0), crate::Env::new(true, 1.0));
        let mut scene = Scene { rt, lifecycle: State::default(), seq: Default::default(), repeat: keys::Repeat::Idle, on_rest: Some(hook) };
        scene.rt.frame(0.0);
        scene.hears(Event::Attached(Phase::Active));
        scene.hears(Event::Inactive);
        assert!(!scene.rt.stale());
        scene.told(Event::Background);
        assert_eq!(scene.rt.state.heard.last(), Some(&"rest"));
        assert_eq!(scene.rt.stale(), stale);
    }
}

/// A view put away before its first frame, as a scene that goes to the background while its
/// app is still starting: the app has heard nothing of its view yet, so it hears neither that
/// it hid nor a rest. Once a frame has run, it hears both.
#[test]
fn an_app_put_away_before_its_first_frame_hears_no_rest() {
    fn visibility(s: &mut Held, visible: bool, _: &mut crate::Tx) { s.heard.push(if visible { "shown" } else { "hidden" }); }
    fn rest(s: &mut Held, _: &mut crate::Tx) -> bool {
        s.heard.push("rest");
        false
    }
    let mut rt = crate::Runtime::new(Held::default(), grip, (100.0, 40.0), crate::Env::new(true, 1.0));
    rt.hooks.on_visibility = Some(visibility);
    rt.set_live_visible(false);
    let mut scene = Scene { rt, lifecycle: State::default(), seq: Default::default(), repeat: keys::Repeat::Idle, on_rest: Some(rest) };
    scene.told(Event::Attached(Phase::Active));
    scene.told(Event::Background);
    assert_eq!(scene.lifecycle, State::Background);
    assert!(scene.rt.state.heard.is_empty(), "{:?}", scene.rt.state.heard);

    scene.hears(Event::Foreground);
    scene.hears(Event::Background);
    assert_eq!(scene.rt.state.heard, ["hidden", "rest"]);
}
