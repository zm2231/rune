//! A physical shortcut editor. The app owns registration and can reject a proposed change.
use std::rc::Rc;
use rune::machine::{Attempt, Effects, Seq};
use rune::{El, Mods, PhysicalInput, PhysicalKeyCode, Shortcut, ShortcutError, Tx, col, row, text};
use crate::{Keybind, Kit};

/// A recorder's value and its capture, as one explicit machine (Desktop's keybind capture,
/// `app/src/settings/keybinds/machine.ts`: idle or capturing). Only a capture has an error, and
/// only an error has the chord it refused.
#[derive(Clone, Debug, Default)]
pub struct ShortcutRecorderState {
    shortcut: Option<Shortcut>,
    phase: Recording,
    /// The modifiers held now, which a capture shows as it waits for a key.
    held: Mods,
    unregistered: bool,
    attempts: Seq,
}

const NOTHING_ARRIVED_SECS: f64 = 5.0;
const LATE_KEY_SECS: f64 = 0.25;
const NOTHING_ARRIVED: &str = "Nothing arrived; that shortcut may be taken by another app.";

#[derive(Clone, Debug, Default)]
enum Recording {
    #[default]
    Idle,
    /// Waiting for a chord; `refusal` is the last one refused, until a new chord starts, and
    /// `finishing` is the key that ended or refused it, whose repeats and release are swallowed.
    Capturing { refusal: Option<Refusal>, finishing: Option<PhysicalKeyCode>, waiting: Option<Waiting> },
    /// The capture is over, and the key that ended it still swallows its repeats and release.
    Swallowing { key: PhysicalKeyCode },
    TimedOut { at: f64 },
}

#[derive(Clone, Copy, Debug)]
struct Waiting {
    attempt: Attempt,
    due: f64,
}

#[derive(Clone, Copy, Debug)]
enum Event {
    Begin { now: f64 },
    KeyArrived { now: f64 },
    Due { attempt: Attempt, now: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Effect {
    WakeAt(f64),
}

impl Recording {
    fn finishing(&self) -> Option<PhysicalKeyCode> {
        match self {
            Recording::Capturing { finishing, .. } => *finishing,
            Recording::Swallowing { key } => Some(*key),
            Recording::Idle | Recording::TimedOut { .. } => None,
        }
    }
    fn finishing_mut(&mut self) -> Option<&mut Option<PhysicalKeyCode>> {
        match self {
            Recording::Capturing { finishing, .. } => Some(finishing),
            Recording::Idle | Recording::Swallowing { .. } | Recording::TimedOut { .. } => None,
        }
    }
    fn refusal_mut(&mut self) -> Option<&mut Option<Refusal>> {
        match self {
            Recording::Capturing { refusal, .. } => Some(refusal),
            Recording::Idle | Recording::Swallowing { .. } | Recording::TimedOut { .. } => None,
        }
    }
}
/// Why a chord was refused, and the chord (None for a refused clear or a key that can't be one).
#[derive(Clone, Debug)]
struct Refusal {
    error: ShortcutError,
    chord: Option<Shortcut>,
}

impl ShortcutRecorderState {
    pub fn new(shortcut: Option<Shortcut>) -> Self { Self { shortcut, ..Self::default() } }
    pub fn shortcut(&self) -> Option<&Shortcut> { self.shortcut.as_ref() }
    pub fn is_recording(&self) -> bool { matches!(self.phase, Recording::Capturing { .. }) }
    fn refusal(&self) -> Option<&Refusal> {
        match &self.phase {
            Recording::Capturing { refusal, .. } => refusal.as_ref(),
            Recording::Idle | Recording::Swallowing { .. } | Recording::TimedOut { .. } => None,
        }
    }
    pub fn error(&self) -> Option<&ShortcutError> { self.refusal().map(|r| &r.error) }
    /// The chord `on_change` just refused, until another chord starts.
    pub fn refused(&self) -> Option<&Shortcut> { self.refusal().and_then(|r| r.chord.as_ref()) }
    /// The shortcut isn't registered (it failed at launch, say): recording the same chord
    /// proposes it through `on_change` again instead of being a no-op, until a change succeeds.
    pub fn mark_unregistered(&mut self) { self.unregistered = true; }
    /// Starts a capture. If no key reaches the window within a few seconds (another app holds
    /// the chord globally, so the system hands it to that app), the capture ends, keeps the
    /// old shortcut and says so; [`Self::poll`] notices the deadline.
    pub fn begin(&mut self, tx: &mut Tx) {
        for effect in self.step(Event::Begin { now: tx.clock() }) {
            match effect { Effect::WakeAt(due) => tx.wake_at(due) }
        }
    }
    /// Call from the app's `poll`: ends a capture whose first key didn't arrive in time.
    /// Returns whether that changed what the recorder shows.
    pub fn poll(&mut self, now: f64) -> bool {
        let Recording::Capturing { waiting: Some(waiting), .. } = self.phase else { return false };
        let _ = self.step(Event::Due { attempt: waiting.attempt, now });
        matches!(self.phase, Recording::TimedOut { .. })
    }
    /// Whether the last capture ended because no key arrived, until the next one starts.
    pub fn timed_out(&self) -> bool { matches!(self.phase, Recording::TimedOut { .. }) }

    fn step(&mut self, event: Event) -> Effects<Effect> {
        match (&mut self.phase, event) {
            (_, Event::Begin { now }) => {
                let waiting = Waiting { attempt: self.attempts.next(), due: now + NOTHING_ARRIVED_SECS };
                self.phase = Recording::Capturing { refusal: None, finishing: None, waiting: Some(waiting) };
                Effects::one(Effect::WakeAt(waiting.due))
            }
            (Recording::Capturing { waiting, .. }, Event::KeyArrived { .. }) => {
                *waiting = None;
                Effects::none()
            }
            (Recording::TimedOut { at }, Event::KeyArrived { now }) if now - *at <= LATE_KEY_SECS => {
                self.phase = Recording::Capturing { refusal: None, finishing: None, waiting: None };
                Effects::none()
            }
            (Recording::Capturing { waiting: Some(waiting), .. }, Event::Due { attempt, now })
                if waiting.attempt == attempt && now >= waiting.due =>
            {
                self.phase = Recording::TimedOut { at: now };
                self.held = Mods::default();
                Effects::none()
            }
            _ => Effects::none(),
        }
    }
    /// Ends the capture, keeping the side of a key that still swallows its repeats and release.
    pub fn cancel(&mut self) {
        self.phase = match self.phase.finishing() {
            Some(key) => Recording::Swallowing { key },
            None => Recording::Idle,
        };
        self.held = Mods::default();
    }
    pub fn keycaps(&self) -> Vec<String> {
        if let Some(refused) = self.refused() { refused.keycaps() }
        else if self.is_recording() { rune::modifier_keycaps(self.held) }
        else { self.shortcut.as_ref().map(Shortcut::keycaps).unwrap_or_default() }
    }

    fn input(&mut self, event: &PhysicalInput, now: f64) -> Outcome {
        match *event {
            PhysicalInput::FocusLost => {
                let active = !matches!(self.phase, Recording::Idle | Recording::TimedOut { .. });
                self.cancel();
                self.phase = Recording::Idle;
                if active { Outcome::Consumed } else { Outcome::Pass }
            }
            PhysicalInput::Modifiers(mods) => {
                self.hold(mods);
                if self.is_recording() { Outcome::Consumed } else { Outcome::Pass }
            }
            PhysicalInput::Key { key, mods, pressed, repeat, .. } => {
                self.hold(mods);
                if self.phase.finishing() == Some(key) {
                    if !pressed {
                        match self.phase.finishing_mut() {
                            Some(finishing) => *finishing = None,
                            None => self.phase = Recording::Idle,
                        }
                    }
                    return Outcome::Consumed;
                }
                let qualifying = pressed && !repeat && !is_modifier(key);
                if qualifying { let _ = self.step(Event::KeyArrived { now }); }
                if !self.is_recording() { return Outcome::Pass; }
                if !qualifying { return Outcome::Consumed; }
                if let Some(finishing) = self.phase.finishing_mut() { *finishing = Some(key); }
                if key == PhysicalKeyCode::Escape { self.cancel(); return Outcome::Consumed; }
                if key == PhysicalKeyCode::Backspace { return Outcome::Change(None); }
                match Shortcut::from_physical_key(key, mods) {
                    Ok(shortcut) => Outcome::Change(Some(shortcut)),
                    Err(error) => { self.refuse(error, None); Outcome::Consumed }
                }
            }
        }
    }

    /// Pressing another modifier starts a new chord: the refused one and its error go.
    fn hold(&mut self, mods: Mods) {
        let held = self.held;
        if (mods.ctrl && !held.ctrl) || (mods.alt && !held.alt) || (mods.shift && !held.shift)
            || (mods.cmd && !held.cmd) || (mods.logo && !held.logo) {
            if let Some(refusal) = self.phase.refusal_mut() { *refusal = None; }
        }
        self.held = mods;
    }

    /// `chord` was refused with `error`: the capture goes on, showing both.
    fn refuse(&mut self, error: ShortcutError, chord: Option<Shortcut>) {
        if let Some(refusal) = self.phase.refusal_mut() { *refusal = Some(Refusal { error, chord }); }
    }

    fn prepare_change(&mut self, event: &PhysicalInput, now: f64, disabled: bool, unmodified: bool) -> PhysicalStep {
        if disabled && !matches!(event, PhysicalInput::FocusLost) {
            return PhysicalStep::Pass;
        }
        match self.input(event, now) {
            Outcome::Pass => PhysicalStep::Pass,
            Outcome::Consumed => PhysicalStep::Consumed,
            Outcome::Change(next) => {
                if let Some(Err(error)) = next.as_ref().filter(|_| !unmodified).map(Shortcut::require_modifier) {
                    self.refuse(error, next);
                    return PhysicalStep::Consumed;
                }
                if self.shortcut == next && !self.unregistered {
                    self.finish_change(next, Ok(()));
                    return PhysicalStep::Consumed;
                }
                PhysicalStep::Propose(next)
            }
        }
    }

    fn finish_change(&mut self, next: Option<Shortcut>, result: Result<(), ShortcutError>) {
        match result {
            Ok(()) => { self.shortcut = next; self.unregistered = false; self.cancel(); }
            Err(error) => self.refuse(error, next),
        }
    }
}

enum Outcome { Pass, Consumed, Change(Option<Shortcut>) }
enum PhysicalStep { Pass, Consumed, Propose(Option<Shortcut>) }
fn is_modifier(key: PhysicalKeyCode) -> bool {
    use PhysicalKeyCode::*;
    matches!(key, ShiftLeft | ShiftRight | ControlLeft | ControlRight | AltLeft | AltRight | SuperLeft | SuperRight | Fn | FnLock)
}
type Access<S> = Rc<dyn for<'a> Fn(&'a mut S) -> &'a mut ShortcutRecorderState>;
type Change<S> = Rc<dyn Fn(&mut S, Option<&Shortcut>, &mut Tx) -> Result<(), ShortcutError>>;
type Message = Box<dyn Fn(&ShortcutError, Option<&Shortcut>) -> String>;

/// The recorder's default error text, for `error_message` closures to fall back on:
/// "⇧⌘K is already in use.", or "Add ⌘, ⌃ or ⌥." for a chord without a modifier. The refused
/// chord is None when clearing was refused.
pub fn shortcut_error_message(error: &ShortcutError, shortcut: Option<&Shortcut>) -> String {
    let Some(shortcut) = shortcut else {
        return if matches!(error, ShortcutError::InvalidAccelerator(_)) { "That key can't be part of a shortcut.".into() }
            else { "The shortcut couldn't be cleared.".into() };
    };
    let chord = shortcut.keycaps().join(if rune::MAC_KEYS { "" } else { "+" });
    match error {
        ShortcutError::InvalidAccelerator(_) if shortcut.require_modifier().is_err() =>
            if rune::MAC_KEYS { "Add ⌘, ⌃ or ⌥.".into() } else { "Add Ctrl, Alt or Super.".into() },
        ShortcutError::Conflict => format!("{chord} is already in use."),
        ShortcutError::Unsupported => "This desktop doesn't support global shortcuts.".into(),
        ShortcutError::PermissionDenied => format!("Permission to use {chord} was declined."),
        _ => format!("{chord} couldn't be registered."),
    }
}

trait RecorderBind {
    fn bind_control(&self, control: El, disabled: bool, unmodified: bool) -> El;
}

struct RecorderHandlers<S> {
    access: Access<S>,
    change: Option<Change<S>>,
}

impl<S: 'static> RecorderBind for RecorderHandlers<S> {
    fn bind_control(&self, mut control: El, disabled: bool, unmodified: bool) -> El {
        if !disabled {
            let access = self.access.clone();
            control = control.on_press_tx(move |s: &mut S, tx| access(s).begin(tx));
        }
        let access = self.access.clone();
        let change = self.change.clone();
        control.on_physical_input(move |s: &mut S, event, tx| match access(s).prepare_change(event, tx.clock(), disabled, unmodified) {
            PhysicalStep::Pass => false,
            PhysicalStep::Consumed => true,
            PhysicalStep::Propose(next) => {
                let result = change.as_ref().map_or(Ok(()), |f| f(s, next.as_ref(), tx));
                access(s).finish_change(next, result);
                true
            }
        })
    }
}

struct RecorderCore {
    key: String,
    state: ShortcutRecorderState,
    message: Message,
    width: f32,
    disabled: bool,
    unmodified: bool,
    label: Option<String>,
}

impl RecorderCore {
    fn new(key: String, state: &ShortcutRecorderState) -> Self {
        Self {
            key,
            state: state.clone(),
            message: Box::new(shortcut_error_message),
            width: 240.0,
            disabled: false,
            unmodified: false,
            label: None,
        }
    }

    fn build(self, k: &Kit, bind: &dyn RecorderBind) -> El {
        let labels = self.state.keycaps();
        let placeholder = if self.state.is_recording() { "Press shortcut" } else { "Record shortcut" };
        let border = if self.state.error().is_some() { k.c.danger.border }
            else if self.state.is_recording() { k.c.border_focus } else { k.c.border_base };
        let keys = if labels.is_empty() { placeholder.to_owned() } else { labels.join(" ") };
        let mut control = k.focus_ring(
            row()
                .key(&self.key)
                .role("button")
                .w(self.width)
                .h(32.0)
                .px(8.0)
                .center_y()
                .radius(k.radius.md)
                .fill(k.c.bg_base)
                .border(1.0, border)
                .focusable(!self.disabled)
                .disabled(self.disabled),
            2.0,
            2.0,
        );
        control = match self.label {
            Some(name) => control.label(name).a11y_value(keys),
            None => control.label(keys),
        };
        if self.state.is_recording() {
            control = control.description("Press a shortcut. Escape cancels; Backspace clears.");
        }
        control = control.child((if labels.is_empty() { text(placeholder).font(k.ty.ui_compact).color(k.c.text_faint) }
            else { Keybind::new(&labels).build(k) }).a11y_hidden());
        control = bind.bind_control(control, self.disabled, self.unmodified);
        let mut root = col().key(format!("{}-field", self.key)).w(self.width).gap(4.0).child(control);
        if let Some(error) = self.state.error() {
            root = root.child(text((self.message)(error, self.state.refused())).key("error").role("alert")
                .font(k.ty.small_medium).color(k.roles.error).wrap().w_full());
        } else if self.state.timed_out() {
            root = root.child(text(NOTHING_ARRIVED).key("nothing-arrived").role("alert")
                .font(k.ty.small_medium).color(k.roles.error).wrap().w_full());
        }
        root
    }
}

/// Click (or activate with Enter/Space) to record. Escape cancels; Backspace clears. Call
/// [`ShortcutRecorderState::poll`] from the app's `poll`: a capture no key reaches within five
/// seconds (another app holds the chord) then ends, keeps the old shortcut and says so.
/// `on_change` must register the proposed binding before releasing the previous registration:
/// returning an error keeps the old value and shows the refused chord with a message
/// (`error_message` words it). An unchanged chord is a no-op unless the state was marked
/// unregistered (`ShortcutRecorderState::mark_unregistered`). A chord without ⌘, ⌃ or ⌥ (Ctrl,
/// Alt or Super elsewhere) other than F1–F24 or Print Screen is refused before `on_change` unless
/// `allow_unmodified`.
pub struct ShortcutRecorder<S> {
    core: RecorderCore,
    handlers: RecorderHandlers<S>,
}
impl<S: 'static> ShortcutRecorder<S> {
    pub fn new(key: impl Into<String>, state: &ShortcutRecorderState,
        access: impl for<'a> Fn(&'a mut S) -> &'a mut ShortcutRecorderState + 'static) -> Self {
        Self { core: RecorderCore::new(key.into(), state), handlers: RecorderHandlers { access: Rc::new(access), change: None } }
    }
    /// The accessible name: what the shortcut does ("Show window"). Assistive technology then
    /// reads the recorded keys as its value. Without it the keys (or "Record shortcut") name it.
    pub fn label(mut self, l: impl Into<String>) -> Self { self.core.label = Some(l.into()); self }
    pub fn on_change(mut self, change: impl Fn(&mut S, Option<&Shortcut>, &mut Tx) -> Result<(), ShortcutError> + 'static) -> Self {
        self.handlers.change = Some(Rc::new(change)); self
    }
    /// Words the error shown under the recorder, given the refused chord (None when clearing
    /// was refused). The default, `shortcut_error_message`, is sentence-case and names the
    /// chord: "⌃J is already in use."
    pub fn error_message(mut self, message: impl Fn(&ShortcutError, Option<&Shortcut>) -> String + 'static) -> Self {
        self.core.message = Box::new(message); self
    }
    /// Propose chords without a modifier too, such as Space or ⇧A. Registering one globally
    /// also needs `App::allow_unmodified_shortcuts`.
    pub fn allow_unmodified(mut self) -> Self { self.core.unmodified = true; self }
    pub fn width(mut self, width: f32) -> Self { self.core.width = width; self }
    pub fn disabled(mut self, disabled: bool) -> Self { self.core.disabled = disabled; self }
    pub fn build(self, k: &Kit) -> El { self.core.build(k, &self.handlers) }
}
