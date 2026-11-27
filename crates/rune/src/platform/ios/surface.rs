//! What a `RuneView` draws: one app's runtime and renderer on the view's `CAMetalLayer`, with the
//! app's state type erased so one Objective-C class serves every app.

#![allow(unsafe_code)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use accesskit::{ActionRequest, TreeUpdate};

use super::appearance::Appearance;
use super::dropped::{self, Kind, Reported};
use super::feedback::Feedback;
use super::field_touch;
use super::keys::Stroke;
use super::lifecycle;
use super::park;
use super::text::doc::Snapshot;
use super::text::geometry::Geometry;
use super::text::undo::Offer;
use crate::a11y::AccessibilityLink;
use crate::app::{OnWindow, Shared};
use crate::embed::{EmbedFrame, LayerHost};
use crate::overlay::ios::{Control, Feed};
use crate::file_dialog::{DialogAnswer, FileDialogError};
use crate::popup_menu::Show;
use crate::runtime::{TouchPhase, Waker};
use crate::time::Instant;
use crate::{GesturePhase, Ime, KeyEvent, MagnifyEvent, PhysicalInput, RotateEvent, WheelEvent};

pub(crate) struct Reading {
    pub(crate) tree: Option<TreeUpdate>,
    pub(crate) masks: Option<Vec<(u64, String)>>,
    pub(crate) first: Option<u64>,
    pub(crate) zoom: f32,
}

/// What the focused field tells the keyboard after a frame.
pub(crate) struct TextState {
    /// A text field, or a caret an app draws itself, has the focus.
    pub(crate) wanted: bool,
    pub(crate) snapshot: Snapshot,
    /// The caret's rect in the view's points.
    pub(crate) caret: Option<[f32; 4]>,
    /// The field is a password.
    pub(crate) secure: bool,
    /// What the field asks of the keyboard; an editor the app draws asks for nothing.
    pub(crate) traits: crate::input::traits::InputTraits,
    /// What a text field with the focus can undo and redo, for the system's undo gestures.
    pub(crate) history: Option<Offer>,
}

pub(crate) trait Surface {
    // Size and frames.
    /// The view's size in points and its display scale.
    fn resize(&mut self, size: (f32, f32), scale: f32);
    /// Steps the app to `target`, when the frame will be on screen, and presents it: with
    /// `only_loops`, when nothing but its loops asked for the frame, a loop-only step may do.
    fn frame(&mut self, target: Instant, refresh_hz: Option<f64>, only_loops: bool) -> EmbedFrame;
    fn presents(&mut self);

    // Touch and pointer.
    /// One finger's event at (x, y) in the view's points, `time` in seconds since boot.
    fn touch(&mut self, id: u64, phase: TouchPhase, x: f32, y: f32, time: f64);
    /// The pointer of a mouse or trackpad is at this point of the view, or left it.
    fn hover(&mut self, at: Option<(f32, f32)>);
    /// A wheel or a trackpad scrolled the content under `at` by `moved` points; `velocity` is
    /// its speed in points a second when the scroll ends.
    fn scroll(&mut self, phase: GesturePhase, at: (f32, f32), moved: (f32, f32), velocity: (f32, f32));

    /// The status bar was tapped: the main list goes to its top. True when there was one to
    /// send.
    fn scroll_to_top(&mut self) -> bool;
    fn main_scroll(&self) -> Option<[f32; 2]>;

    // Gestures.
    /// Two fingers pinched: scale by `1.0 + delta` about (x, y) in the view's points.
    fn magnify(&mut self, delta: f32, x: f32, y: f32, phase: GesturePhase);
    fn touch_pan(&mut self, phase: GesturePhase, at: (f32, f32), moved: (f32, f32));
    /// Two fingers turned `degrees`, positive counterclockwise, about (x, y).
    fn rotate(&mut self, degrees: f32, x: f32, y: f32, phase: GesturePhase);
    /// Whether a secondary press at (x, y) would open a menu or reach a handler.
    fn takes_secondary(&self, x: f32, y: f32) -> bool;
    fn on_focused_field(&self, x: f32, y: f32) -> bool;
    /// A secondary press at (x, y): a long press.
    fn secondary(&mut self, x: f32, y: f32);

    // Menus and dialogs.
    /// The next pop-up menu to show, once the frame that asked for it is done.
    fn popup(&mut self) -> Option<(Show, bool)>;
    /// The menu showing was answered with an item's id, or dismissed.
    fn popup_pick(&mut self, picked: Option<String>);
    /// The file dialog to show, once the frame that asked for it is done.
    fn file_dialog(&mut self) -> Option<crate::file_dialog::Shown>;
    fn share(&mut self) -> Option<crate::share::Shown>;
    /// The dialog's answer: the chosen file, none when cancelled, or why it could not show.
    fn file_dialog_answer(&mut self, id: u64, answer: Result<DialogAnswer, String>);

    // Keys and text.
    /// A key from the soft keyboard, or an edit action UIKit sent.
    fn key(&mut self, key: KeyEvent);
    /// A hardware key going down (`pressed`, held when `repeat`) or up.
    fn stroke(&mut self, stroke: Stroke, pressed: bool, repeat: bool);
    /// A composition, a commit or a selection from the keyboard.
    fn ime(&mut self, ime: Ime);
    /// What the focused field tells the keyboard, as of the last frame.
    fn text(&self) -> TextState;
    /// Where the focused field's characters are, as last drawn.
    fn text_geometry(&self) -> Option<Geometry>;

    // Accessibility.
    /// An assistive client asked for the view's elements: the runtime builds its tree, which
    /// comes back at once when a frame has drawn the view.
    fn accessibility_wanted(&mut self) -> Option<Reading>;
    /// What a client did to an element, for the next frame.
    fn accessibility_act(&mut self, request: ActionRequest);
    /// The accessibility tree's changes since the last frame, while a client listens.
    fn accessibility(&mut self) -> Option<Reading>;

    // The app's lifecycle.
    /// The view joined or left a window, or its scene or app changed phase: the app's hooks
    /// hear what changed for it (`lifecycle`). True when held keys were released, for the
    /// view to stop its key repeat.
    fn lifecycle(&mut self, event: lifecycle::Event) -> bool;

    // System settings.
    /// The system's appearance.
    fn set_dark(&mut self, dark: bool);
    /// The view's tint, the app's accent. True when it is not the accent the app had.
    fn set_accent(&mut self, accent: crate::SystemAccent) -> bool;
    /// The Reduce Motion setting.
    fn set_reduced_motion(&mut self, reduced: bool);
    /// The text size setting, as a scale of its default.
    fn set_text_scale(&mut self, scale: f32);
    fn set_bold_text(&mut self, on: bool);
    fn set_increased_contrast(&mut self, on: bool);
    fn set_size_classes(&mut self, classes: crate::SizeClasses);
    /// What the screen keeps at the view's top, right, bottom and left edges, in points.
    fn set_safe_area(&mut self, insets: [f32; 4]);
    fn set_keyboard(&mut self, visible: Option<(f32, f32)>);
    /// The system is short of memory: the caches the last frame didn't draw from go, and
    /// `App::on_rest` runs for the app's own. True when the hook left a frame work to do.
    fn memory_warning(&mut self, on_screen: bool) -> bool;
    fn interface_style(&mut self) -> Option<isize>;

    fn takes_touch(&self, control: &Control, x: f32, y: f32) -> bool;
    fn take_overlays(&mut self) -> Vec<OverlaySlot>;
    fn hidden_parts(&mut self) -> crate::overlay::ios::Mounted<'_>;
    fn set_window(&mut self, key: String);
    fn close(&mut self);

    fn enter_turn(&mut self);
    fn leave_turn(&mut self);
}

pub(super) fn in_turn<R>(surface: &mut dyn Surface, f: impl FnOnce(&mut dyn Surface) -> R) -> R {
    surface.enter_turn();
    let result = f(&mut *surface);
    surface.leave_turn();
    result
}

pub(crate) struct OverlaySlot {
    pub(crate) control: Rc<RefCell<Control>>,
    pub(crate) mount: Mount,
    pub(crate) standby: crate::overlay::ios::Standby,
}

pub(super) struct Hosted<S: 'static> {
    pub(super) host: LayerHost<S>,
    pub(super) role: Role<S>,
    pub(super) accessibility: AccessibilityLink,
    /// The app's name, which names the accessibility tree's root.
    pub(super) title: &'static str,
    /// What the app's hooks were last told of the view's scene.
    pub(super) lifecycle: lifecycle::State,
    /// What a finger on the focused field is doing, and the field's menu.
    pub(super) field: field_touch::State,
    pub(super) wake: Waker,
    pub(super) appearance: Appearance,
    pub(super) park: park::State,
    pub(super) feedback: Feedback,
    pub(super) member: Option<crate::shared_state::Member<S>>,
}

impl<S: 'static> Drop for Hosted<S> {
    fn drop(&mut self) {
        if let Some(member) = &mut self.member { member.release(self.host.runtime_mut()); }
    }
}

pub(super) enum Role<S: 'static> {
    App(Box<AppRole<S>>),
    Overlay(Box<OverlayRole<S>>),
}

pub(super) struct AppRole<S: 'static> {
    /// What the app's handles reach it through: `AppHandle::update` and `wake` from any thread.
    pub(super) shared: Arc<Shared<S>>,
    /// The app's menu items, in UIKit's main menu.
    pub(super) menus: crate::native_menu::MenuGuard,
    /// `App::on_ready`, which runs once the first frame is on screen, as a window's does.
    pub(super) on_ready: Option<OnWindow>,
    /// The kinds of request this view has said it drops.
    pub(super) dropped: Reported,
    pub(super) on_quit: Option<fn(&mut S, &mut crate::Tx)>,
    pub(super) on_rest: Option<fn(&mut S, &mut crate::Tx) -> bool>,
    pub(super) on_close: Option<fn(&mut S, &mut crate::Tx)>,
    /// Its place among the apps receiving links (`App::on_open_url`).
    pub(super) _open_urls: crate::open_url::incoming::Listeners,
    pub(super) window: Option<String>,
    pub(super) undrawn: bool,
    pub(super) overlays: Vec<OverlaySlot>,
}

pub(super) struct OverlayRole<S: 'static> {
    pub(super) feed: Feed<S>,
}

impl<S: 'static> Role<S> {
    fn on_rest(&self) -> Option<fn(&mut S, &mut crate::Tx) -> bool> {
        match self {
            Role::App(app) => app.on_rest,
            Role::Overlay(_) => None,
        }
    }
}

impl<S: 'static> Hosted<S> {
    /// Steps the focused field's finger and menu, and does what the step asks for.
    fn field_step(&mut self, event: field_touch::Event) {
        for effect in field_touch::step(&mut self.field, event) {
            match effect {
                field_touch::Effect::Select(range) => self.host.runtime_mut().ime(Ime::Select(range)),
                field_touch::Effect::Press((x, y)) => self.secondary(x, y),
                field_touch::Effect::Wake => (self.wake)(),
            }
        }
    }

    fn park_step(&mut self, event: park::Event) -> bool {
        let mut work = false;
        for effect in park::step(&mut self.park, event) {
            match effect {
                park::Effect::Trim => work |= self.host.trim_memory_with(self.role.on_rest()),
                park::Effect::Release => {
                    self.feedback.reset();
                    self.host.release_gpu();
                }
                park::Effect::Remake => if let Err(error) = self.host.remake_gpu() { eprintln!("rune: a view could not make its renderer again: {error}") },
            }
        }
        work
    }

    fn app_requests(&mut self) {
        let Role::App(app) = &mut self.role else { return };
        let host = &mut self.host;
        let requests = app.shared.drain(|update| { host.runtime_mut().update_if(update); });
        if let Some(key) = &app.window {
            super::scenes::asked(key, requests.window.clone(), requests.activation == Some(crate::app::Activation::Focus));
        }
        for (kind, asked) in dropped::asked(&requests, app.window.is_some()) {
            if asked { report(&mut app.dropped, kind); }
        }
        if let Some(scheme) = requests.color_scheme { self.appearance.set_app(scheme); }
        app.menus.drain();
    }

    fn app_frame_done(&mut self, frame: EmbedFrame) {
        let rt = self.host.runtime_mut();
        if let Some(scheme) = rt.host.window_color_scheme.take() { self.appearance.set_window(scheme); }
        let (shell, minimize) = (rt.shell_request.take(), rt.host.minimize.field());
        if minimize { rt.minimize_step(crate::window::MinimizeEvent::Lost); }
        let Role::App(app) = &mut self.role else { return };
        let managed = app.window.is_some();
        if let Some(key) = &app.window && shell == Some(crate::Shell::Close) {
            super::scenes::asked(key, Some(crate::app::WindowRequest::Close), false);
        }
        let opened = if managed { 0 } else { dropped::take_opened() };
        for (kind, asked) in dropped::asked_by_handlers(shell, minimize, opened, managed) {
            if asked { report(&mut app.dropped, kind); }
        }
        if !frame.drew { return; }
        if let Some(key) = app.window.as_deref().filter(|_| app.undrawn) {
            app.undrawn = false;
            super::scenes::drawn(key);
        }
        if let Some(mut ready) = app.on_ready.take() { ready(&crate::WindowHandle::new(app.window.as_deref().unwrap_or("main"))); }
    }
}

impl<S: 'static> Surface for Hosted<S> {
    // Size and frames.
    fn resize(&mut self, size: (f32, f32), scale: f32) { self.host.resize(size, scale); }

    fn frame(&mut self, target: Instant, refresh_hz: Option<f64>, only_loops: bool) -> EmbedFrame {
        self.accessibility.receive(self.host.runtime_mut().core_mut(), self.title);
        self.park_step(park::Event::Drawing);
        self.app_requests();
        if let Role::Overlay(overlay) = &self.role { overlay.feed.drain(self.host.runtime_mut()); }
        self.host.runtime_mut().core_mut().take_late_link_answers();
        let summary = self.host.frame_summary(Some(target), only_loops);
        if summary.drawn.is_some() { crate::marks::presented(); }
        self.feedback.frame(&summary, target, refresh_hz);
        let frame = EmbedFrame::from(summary);
        match &mut self.role {
            Role::App(_) => self.app_frame_done(frame),
            Role::Overlay(overlay) => overlay.feed.fit(self.host.runtime_mut()),
        }
        // A tap that left the field's caret where it was, or Select All, opens the field's menu.
        if self.field.awaits_frame() {
            let rt = self.host.runtime();
            self.field_step(field_touch::Event::Frame { text: rt.core().ime_text(), popup: rt.core().popup_menu().is_some() });
        }
        frame
    }

    fn presents(&mut self) { self.feedback.drain(); }

    // Touch and pointer.
    fn touch(&mut self, id: u64, phase: TouchPhase, x: f32, y: f32, time: f64) {
        let (vx, vy) = self.in_view(x, y);
        let rt = self.host.runtime();
        // The field as it is before the runtime hears of the finger, which places the caret.
        let event = if phase == TouchPhase::Started && !rt.touching() {
            field_touch::Event::Landed { at: (x, y), text: rt.core().ime_text(), on_field: rt.core().on_focused_field(vx, vy) }
        } else {
            field_touch::Event::Touch { phase, at: (x, y) }
        };
        self.field_step(event);
        self.host.runtime_mut().touch(id, phase, vx, vy, time);
    }

    fn hover(&mut self, at: Option<(f32, f32)>) {
        let at = at.map(|(x, y)| self.in_view(x, y));
        let rt = self.host.runtime_mut();
        // A finger's touch is the pointer while it is down.
        if rt.touching() { return; }
        match at {
            Some((x, y)) => rt.pointer_move(x, y),
            None => rt.pointer_leave(),
        }
    }

    fn scroll(&mut self, phase: GesturePhase, (x, y): (f32, f32), (dx, dy): (f32, f32), (vx, vy): (f32, f32)) {
        // A scroll moves the content as far on screen at any zoom, as a desktop wheel does.
        let (x, y) = self.in_view(x, y);
        let ((dx, dy), (vx, vy)) = (self.in_view(dx, dy), self.in_view(vx, vy));
        let rt = self.host.runtime_mut();
        match phase {
            GesturePhase::Began => { rt.wheel_began(); rt.wheel_xy(x, y, dx, dy); }
            GesturePhase::Moved => rt.wheel_xy(x, y, dx, dy),
            GesturePhase::Ended => {
                rt.wheel_xy(x, y, dx, dy);
                rt.wheel_ended();
                // UIKit sends no momentum after the fingers lift: the runtime glides it.
                if vx != 0.0 || vy != 0.0 { rt.fling_xy(x, y, vx, vy); }
            }
        }
    }

    fn scroll_to_top(&mut self) -> bool { self.host.runtime_mut().scroll_main_to_top() }
    fn main_scroll(&self) -> Option<[f32; 2]> { self.host.runtime().main_scroll() }

    // Gestures.
    fn magnify(&mut self, delta: f32, x: f32, y: f32, phase: GesturePhase) {
        let (x, y) = self.in_view(x, y);
        self.host.runtime_mut().magnify_event(MagnifyEvent::new(delta).at(x, y).phase(phase));
    }

    fn touch_pan(&mut self, phase: GesturePhase, (x, y): (f32, f32), (dx, dy): (f32, f32)) {
        let ((x, y), (dx, dy)) = (self.in_view(x, y), self.in_view(dx, dy));
        self.host.runtime_mut().touch_pan_event(WheelEvent::new(dx, dy).at(x, y).phase(phase).precise(true));
    }

    fn rotate(&mut self, degrees: f32, x: f32, y: f32, phase: GesturePhase) {
        let (x, y) = self.in_view(x, y);
        self.host.runtime_mut().rotate_event(RotateEvent::new(degrees).at(x, y).phase(phase));
    }

    fn takes_secondary(&self, x: f32, y: f32) -> bool {
        let (x, y) = self.in_view(x, y);
        self.host.runtime().core().takes_secondary(x, y)
    }

    fn on_focused_field(&self, x: f32, y: f32) -> bool {
        let (x, y) = self.in_view(x, y);
        self.host.runtime().core().on_focused_field(x, y)
    }

    fn secondary(&mut self, x: f32, y: f32) {
        let (x, y) = self.in_view(x, y);
        // A finger that lands in a field places the caret at once, as a mouse press does, so a
        // long press on selected text would find nothing selected. The selection the finger
        // landed on is put back first; the field's menu then keeps it for a press inside it
        // and moves the caret for one outside, as for a right-click.
        let text = self.host.runtime().core().ime_text();
        self.field_step(field_touch::Event::Pressing { text });
        let rt = self.host.runtime_mut();
        rt.secondary_down(x, y);
        rt.secondary_up(x, y);
    }

    // Menus and dialogs.
    fn popup(&mut self) -> Option<(Show, bool)> {
        let rt = self.host.runtime_mut();
        if rt.core().popup_waits_for_layout() { return None; }
        let field = rt.core().popup_is_field_menu();
        match rt.core_mut().take_popup()? {
            Some(show) => {
                self.field_step(field_touch::Event::Shown { at: show.at, field });
                Some((show, field))
            }
            // It can't open: its anchor is gone.
            None => { rt.resolve_popup(None).ok(); None }
        }
    }

    fn popup_pick(&mut self, picked: Option<String>) {
        self.host.runtime_mut().resolve_popup(picked.as_deref()).ok();
        self.field_step(field_touch::Event::Answered { select_all: picked.as_deref() == Some("select-all") });
    }

    fn share(&mut self) -> Option<crate::share::Shown> {
        let wake = self.wake.clone();
        self.host.runtime_mut().core_mut().take_share(wake)
    }

    fn file_dialog(&mut self) -> Option<crate::file_dialog::Shown> {
        self.host.runtime_mut().core_mut().take_file_dialog()
    }

    fn file_dialog_answer(&mut self, id: u64, answer: Result<DialogAnswer, String>) {
        self.host.runtime_mut().core_mut().answer_file_dialog(id, answer.map_err(FileDialogError));
    }

    // Keys and text.
    fn key(&mut self, key: KeyEvent) { self.host.key(key); }

    fn stroke(&mut self, stroke: Stroke, pressed: bool, repeat: bool) {
        let rt = self.host.runtime_mut();
        let Stroke { code, mods, key, base } = stroke;
        match code {
            // The physical key first, with the logical one for when no handler takes it. A
            // release carries its logical key too, for `on_key_up`.
            Some(code) => {
                // A `keys_first` menu item's chord runs the item when no handler takes it.
                let unhandled = key.as_ref().filter(|_| pressed).and_then(crate::native_menu::keys_first);
                rt.physical_then(PhysicalInput::Key { key: code, mods, pressed, repeat, text: None, consumed_mods: Default::default() }, key, base, unhandled);
            }
            // A key with no physical code still edits and moves.
            None => if let Some(key) = key.filter(|_| pressed) { self.host.key(key) },
        }
    }

    fn ime(&mut self, ime: Ime) { self.host.runtime_mut().ime(ime); }

    fn text(&self) -> TextState {
        let rt = self.host.runtime();
        let secure = rt.has_secure_focus();
        let snapshot = match rt.core().ime_text() {
            _ if secure => Snapshot::Hidden,
            _ if rt.core().focused_preedit().is_some() => Snapshot::Composing,
            Some(text) => Snapshot::Field { text: text.text, start: text.start, selection: text.selection },
            None => Snapshot::Hidden,
        };
        let traits = rt.core().focused_input_traits().unwrap_or_default();
        let history = rt.core().focused_history().map(|(undo, redo)| Offer { undo, redo });
        TextState { wanted: rt.core().wants_text(), snapshot, caret: rt.ime_area.map(|area| rt.to_window(area)), secure, traits, history }
    }

    fn text_geometry(&self) -> Option<Geometry> {
        self.host.runtime().core().focused_field_lines().map(|lines| Geometry::new(&lines))
    }

    // Accessibility.
    fn accessibility_wanted(&mut self) -> Option<Reading> {
        self.accessibility.activate();
        self.accessibility.receive(self.host.runtime_mut().core_mut(), self.title);
        self.accessibility()
    }

    fn accessibility_act(&mut self, request: ActionRequest) { self.accessibility.act(request); }

    fn accessibility(&mut self) -> Option<Reading> {
        let zoom = self.host.runtime().zoom();
        let tree = self.accessibility.update(self.host.runtime_mut().core_mut());
        let core = self.host.runtime().core();
        let masks = (tree.is_some() || (core.has_secure_focus() && core.a11y_listening())).then(|| core.a11y_masks());
        let first = tree.as_ref().and_then(|_| core.a11y_first());
        (tree.is_some() || masks.is_some()).then_some(Reading { tree, masks, first, zoom })
    }

    // The app's lifecycle.
    fn lifecycle(&mut self, event: lifecycle::Event) -> bool {
        let mut released = false;
        for effect in lifecycle::step(&mut self.lifecycle, event) {
            let rt = self.host.runtime_mut();
            match effect {
                // As a window going off screen and back: live values rest, and
                // `App::on_visibility` hears each change after the first frame.
                lifecycle::Effect::Visibility(visible) => {
                    if let Role::App(app) = &self.role { app.shared.visible.store(visible, Ordering::Release); }
                    rt.set_live_visible(visible);
                }
                // As a window's focus event: `App::on_focus` runs, and a blur ends what the
                // pointer had under way and then releases the held keys
                // (`PhysicalInput::FocusLost`), in that order. The keyboard keeps an open
                // composition.
                lifecycle::Effect::Focus(focused) => {
                    rt.scene_focus(focused);
                    released |= !focused;
                }
                // As a window out of sight: everything kept between frames goes, and
                // `App::on_rest` runs. Nothing here encodes or submits GPU work, which the
                // background forbids, and no frame follows until the scene is in front again:
                // a hook that changed state leaves the runtime's next frame to show it.
                lifecycle::Effect::Rest => self.host.rest(self.role.on_rest(), true),
                lifecycle::Effect::Quit => {
                    if let Role::App(app) = &self.role && let Some(quit) = app.on_quit { rt.update(quit); }
                    // The hook runs inside the notification. No frame follows it, so what it
                    // copied or cleared goes to the clipboard's worker here; the process may
                    // end before the worker has written it.
                    self.host.flush_clipboard();
                }
            }
        }
        released
    }

    // System settings.

    fn set_dark(&mut self, dark: bool) { self.host.runtime_mut().set_system_dark(dark); }

    fn set_accent(&mut self, accent: crate::SystemAccent) -> bool { self.host.runtime_mut().set_system_accent(accent) }

    fn set_reduced_motion(&mut self, reduced: bool) { self.host.runtime_mut().set_system_reduced_motion(reduced); }

    fn set_text_scale(&mut self, scale: f32) { self.host.runtime_mut().set_system_text_scale(scale); }

    fn set_bold_text(&mut self, on: bool) { self.host.runtime_mut().set_system_bold_text(on); }

    fn set_increased_contrast(&mut self, on: bool) { self.host.runtime_mut().set_system_increased_contrast(on); }

    fn set_size_classes(&mut self, classes: crate::SizeClasses) { self.host.runtime_mut().set_size_classes(classes); }

    fn set_safe_area(&mut self, insets: [f32; 4]) {
        let rt = self.host.runtime_mut();
        rt.shift_main_scroll(insets[0] - rt.core().safe_area()[0]);
        rt.set_safe_area(insets);
    }

    fn set_keyboard(&mut self, visible: Option<(f32, f32)>) {
        let core = self.host.runtime_mut().core_mut();
        core.set_visual_viewport_y(visible);
        if visible.is_some() { core.request_caret_follow(); }
    }

    fn memory_warning(&mut self, on_screen: bool) -> bool { self.park_step(park::Event::MemoryWarning { on_screen }) }

    fn interface_style(&mut self) -> Option<isize> { self.appearance.changed() }

    fn takes_touch(&self, control: &Control, x: f32, y: f32) -> bool {
        control.takes(self.host.runtime().hit_regions(), x, y)
    }

    fn hidden_parts(&mut self) -> crate::overlay::ios::Mounted<'_> { self.host.hidden_parts() }

    fn take_overlays(&mut self) -> Vec<OverlaySlot> {
        match &mut self.role {
            Role::App(app) => std::mem::take(&mut app.overlays),
            Role::Overlay(_) => Vec::new(),
        }
    }

    fn set_window(&mut self, key: String) {
        if let Role::App(app) = &mut self.role { app.window = Some(key); }
    }

    fn close(&mut self) {
        if let Role::App(app) = &self.role && let Some(close) = app.on_close { self.host.runtime_mut().update(close); }
        (self.wake)();
    }

    fn enter_turn(&mut self) {
        if let Some(member) = &mut self.member { member.enter(self.host.runtime_mut()); }
    }

    fn leave_turn(&mut self) {
        if let Some(member) = &mut self.member { member.leave(self.host.runtime_mut()); }
    }
}

/// Starts an app on a `CAMetalLayer`, once its view has a size: the layer, the view's size in
/// points, its scale, whether the system is dark, the waker other threads call, and the one a
/// frame's present time calls when it lands.
pub(crate) type Mount = Box<dyn FnOnce(*mut std::ffi::c_void, (f32, f32), f32, bool, Waker, Waker) -> Result<Box<dyn Surface>, String>>;

impl<S: 'static> Hosted<S> {
    /// A point of the `UIView` in the app's own points (`zoom`).
    fn in_view(&self, x: f32, y: f32) -> (f32, f32) {
        super::zoom::from_window(self.host.runtime().zoom(), x, y)
    }
}

/// Says on stderr that a request of this kind is dropped, the first time the view drops one.
pub(super) fn report(reported: &mut Reported, kind: Kind) {
    if reported.first(kind) { eprintln!("rune: {}", kind.message()); }
}
