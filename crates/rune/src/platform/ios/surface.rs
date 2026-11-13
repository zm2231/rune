//! What a `RuneView` draws: one app's runtime and renderer on the view's `CAMetalLayer`, with the
//! app's state type erased so one Objective-C class serves every app.

use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use accesskit::{ActionRequest, TreeUpdate};

use super::dropped::{self, Kind, Reported};
use super::field_touch;
use super::keys::Stroke;
use super::lifecycle;
use super::text::doc::Snapshot;
use super::text::geometry::Geometry;
use crate::a11y::AccessibilityLink;
use crate::app::{App, OnWindow, Shared};
use crate::embed::{EmbedFrame, LayerHost};
use crate::file_dialog::{FileDialogError, FileDialogKind, FileDialogOptions};
use crate::popup_menu::Show;
use crate::runtime::{TouchPhase, Waker};
use crate::time::Instant;
use crate::{GesturePhase, Ime, KeyEvent, MagnifyEvent, PhysicalInput, RotateEvent};

/// What the focused field tells the keyboard after a frame.
pub(crate) struct TextState {
    /// A text field, or a caret an app draws itself, has the focus.
    pub(crate) wanted: bool,
    pub(crate) snapshot: Snapshot,
    /// The caret's rect in the view's points.
    pub(crate) caret: Option<[f32; 4]>,
    /// The field is a password.
    pub(crate) secure: bool,
}

pub(crate) trait Surface {
    // Size and frames.
    /// The view's size in points and its display scale.
    fn resize(&mut self, size: (f32, f32), scale: f32);
    /// Steps the app to `target`, when the frame will be on screen, and presents it.
    fn frame(&mut self, target: Instant) -> EmbedFrame;

    // Touch and pointer.
    /// One finger's event at (x, y) in the view's points, `time` in seconds since boot.
    fn touch(&mut self, id: u64, phase: TouchPhase, x: f32, y: f32, time: f64);
    /// The pointer of a mouse or trackpad is at this point of the view, or left it.
    fn hover(&mut self, at: Option<(f32, f32)>);
    /// A wheel or a trackpad scrolled the content under `at` by `moved` points; `velocity` is
    /// its speed in points a second when the scroll ends.
    fn scroll(&mut self, phase: GesturePhase, at: (f32, f32), moved: (f32, f32), velocity: (f32, f32));

    // Gestures.
    /// Two fingers pinched: scale by `1.0 + delta` about (x, y) in the view's points.
    fn magnify(&mut self, delta: f32, x: f32, y: f32, phase: GesturePhase);
    /// Two fingers turned `degrees`, positive counterclockwise, about (x, y).
    fn rotate(&mut self, degrees: f32, x: f32, y: f32, phase: GesturePhase);
    /// Whether a secondary press at (x, y) would open a menu or reach a handler.
    fn takes_secondary(&self, x: f32, y: f32) -> bool;
    /// A secondary press at (x, y): a long press.
    fn secondary(&mut self, x: f32, y: f32);

    // Menus and dialogs.
    /// The next pop-up menu to show, once the frame that asked for it is done.
    fn popup(&mut self) -> Option<(Show, bool)>;
    /// The menu showing was answered with an item's id, or dismissed.
    fn popup_pick(&mut self, picked: Option<String>);
    /// The file dialog to show, once the frame that asked for it is done.
    fn file_dialog(&mut self) -> Option<(u64, FileDialogKind, FileDialogOptions)>;
    /// The dialog's answer: the chosen file, none when cancelled, or why it could not show.
    fn file_dialog_answer(&mut self, id: u64, answer: Result<Option<PathBuf>, String>);

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
    /// An assistive client asked for the view's elements: the runtime builds its tree.
    fn accessibility_wanted(&mut self);
    /// What a client did to an element, for the next frame.
    fn accessibility_act(&mut self, request: ActionRequest);
    /// The accessibility tree's changes since the last frame, while a client listens, and the
    /// view's zoom: the tree is in the app's points, which the zoom scales to the view's.
    fn accessibility(&mut self) -> Option<(TreeUpdate, f32)>;

    // The app's lifecycle.
    /// The view joined or left a window, or its scene or app changed phase: the app's hooks
    /// hear what changed for it (`lifecycle`). True when held keys were released, for the
    /// view to stop its key repeat.
    fn lifecycle(&mut self, event: lifecycle::Event) -> bool;

    // System settings.
    /// The system's appearance.
    fn set_dark(&mut self, dark: bool);
    /// The Reduce Motion setting.
    fn set_reduced_motion(&mut self, reduced: bool);
    /// The text size setting, as a scale of its default.
    fn set_text_scale(&mut self, scale: f32);
    /// What the screen keeps at the view's top, right, bottom and left edges, in points.
    fn set_safe_area(&mut self, insets: [f32; 4]);
    /// The system is short of memory: the caches the last frame didn't draw from go, and
    /// `App::on_rest` runs for the app's own. True when the hook left a frame work to do.
    fn memory_warning(&mut self) -> bool;
}

struct Hosted<S: 'static> {
    host: LayerHost<S>,
    /// What the app's handles reach it through: `AppHandle::update` and `wake` from any thread.
    shared: Arc<Shared<S>>,
    /// The app's menu items, in UIKit's main menu.
    menus: crate::native_menu::MenuGuard,
    accessibility: AccessibilityLink,
    /// The app's name, which names the accessibility tree's root.
    title: &'static str,
    /// `App::on_ready`, which runs once the first frame is on screen, as a window's does.
    on_ready: Option<OnWindow>,
    /// The kinds of request this view has said it drops.
    dropped: Reported,
    /// `App::on_quit`, which runs when the app is about to terminate.
    on_quit: Option<fn(&mut S, &mut crate::Tx)>,
    /// `App::on_rest`, which runs when the view rests: its scene went to the background, or
    /// it left its window.
    on_rest: Option<fn(&mut S, &mut crate::Tx) -> bool>,
    /// What the app's hooks were last told of the view's scene.
    lifecycle: lifecycle::State,
    /// What a finger on the focused field is doing, and the field's menu.
    field: field_touch::State,
    /// Its place among the apps receiving links (`App::on_open_url`).
    _open_urls: Option<crate::open_url::incoming::Listening>,
    wake: Waker,
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
}

impl<S: 'static> Surface for Hosted<S> {
    // Size and frames.
    fn resize(&mut self, size: (f32, f32), scale: f32) { self.host.resize(size, scale); }

    fn frame(&mut self, target: Instant) -> EmbedFrame {
        self.accessibility.receive(self.host.runtime_mut().core_mut(), self.title);
        // What the app's handles asked since the last frame. A view is not a window: requests
        // to show, move or quit one have nothing to act on, and only the updates run. The
        // first dropped request of each kind says so.
        let host = &mut self.host;
        let requests = self.shared.drain(|update| { host.runtime_mut().update_if(update); });
        for (kind, asked) in dropped::asked(&requests) {
            if asked { report(&mut self.dropped, kind); }
        }
        // An update may have set the menu (`AppHandle::set_menu`).
        self.menus.drain();
        let frame = self.host.frame_at(target);
        // What handlers asked of a window in this frame, or in a hook since the last, is
        // taken and dropped the same way: none of it waits or piles up.
        let rt = self.host.runtime_mut();
        let (shell, minimize) = (rt.shell_request.take(), rt.host.minimize.field());
        if minimize { rt.minimize_step(crate::window::MinimizeEvent::Lost); }
        for (kind, asked) in dropped::asked_by_handlers(shell, minimize, dropped::take_opened()) {
            if asked { report(&mut self.dropped, kind); }
        }
        // There is no window to hand over: the handle is one no window answers, as on the web.
        if frame.drew && let Some(mut ready) = self.on_ready.take() { ready(&crate::WindowHandle::new("main")); }
        // A tap that left the field's caret where it was, or Select All, opens the field's menu.
        if self.field.awaits_frame() {
            let rt = self.host.runtime();
            self.field_step(field_touch::Event::Frame { text: rt.core().ime_text(), popup: rt.core().popup_menu().is_some() });
        }
        frame
    }

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

    // Gestures.
    fn magnify(&mut self, delta: f32, x: f32, y: f32, phase: GesturePhase) {
        let (x, y) = self.in_view(x, y);
        self.host.runtime_mut().magnify_event(MagnifyEvent::new(delta).at(x, y).phase(phase));
    }

    fn rotate(&mut self, degrees: f32, x: f32, y: f32, phase: GesturePhase) {
        let (x, y) = self.in_view(x, y);
        self.host.runtime_mut().rotate_event(RotateEvent::new(degrees).at(x, y).phase(phase));
    }

    fn takes_secondary(&self, x: f32, y: f32) -> bool {
        let (x, y) = self.in_view(x, y);
        self.host.runtime().core().takes_secondary(x, y)
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
            None => { let _ = rt.resolve_popup(None); None }
        }
    }

    fn popup_pick(&mut self, picked: Option<String>) {
        let _ = self.host.runtime_mut().resolve_popup(picked.as_deref());
        self.field_step(field_touch::Event::Answered { select_all: picked.as_deref() == Some("select-all") });
    }

    fn file_dialog(&mut self) -> Option<(u64, FileDialogKind, FileDialogOptions)> {
        self.host.runtime_mut().core_mut().take_file_dialog()
    }

    fn file_dialog_answer(&mut self, id: u64, answer: Result<Option<PathBuf>, String>) {
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
                rt.physical_then(PhysicalInput::Key { key: code, mods, pressed, repeat }, key, base, unhandled);
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
        TextState { wanted: rt.core().wants_text(), snapshot, caret: rt.ime_area.map(|area| rt.to_window(area)), secure }
    }

    fn text_geometry(&self) -> Option<Geometry> {
        self.host.runtime().core().focused_field_lines().map(|lines| Geometry::new(&lines))
    }

    // Accessibility.
    fn accessibility_wanted(&mut self) { self.accessibility.activate(); }

    fn accessibility_act(&mut self, request: ActionRequest) { self.accessibility.act(request); }

    fn accessibility(&mut self) -> Option<(TreeUpdate, f32)> {
        let zoom = self.host.runtime().zoom();
        self.accessibility.update(self.host.runtime_mut().core_mut()).map(|update| (update, zoom))
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
                    self.shared.visible.store(visible, Ordering::Release);
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
                lifecycle::Effect::Rest => self.host.rest(self.on_rest, true),
                lifecycle::Effect::Quit => {
                    if let Some(quit) = self.on_quit { rt.update(quit); }
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

    fn set_reduced_motion(&mut self, reduced: bool) { self.host.runtime_mut().set_system_reduced_motion(reduced); }

    fn set_text_scale(&mut self, scale: f32) { self.host.runtime_mut().set_system_text_scale(scale); }

    fn set_safe_area(&mut self, insets: [f32; 4]) { self.host.runtime_mut().set_safe_area(insets); }

    fn memory_warning(&mut self) -> bool { self.host.trim_memory_with(self.on_rest) }
}

/// Starts an app on a `CAMetalLayer`, once its view has a size: the layer, the view's size in
/// points, its scale, whether the system is dark, and the waker other threads call.
pub(crate) type Mount = Box<dyn FnOnce(*mut c_void, (f32, f32), f32, bool, Waker) -> Result<Box<dyn Surface>, String>>;

impl<S: 'static> Hosted<S> {
    /// A point of the `UIView` in the app's own points (`zoom`).
    fn in_view(&self, x: f32, y: f32) -> (f32, f32) {
        super::zoom::from_window(self.host.runtime().zoom(), x, y)
    }
}

/// Says on stderr that a request of this kind is dropped, the first time the view drops one.
fn report(reported: &mut Reported, kind: Kind) {
    if reported.first(kind) { eprintln!("rune: {}", kind.message()); }
}

pub(crate) fn mount<S: 'static>(mut app: App<S>) -> Mount {
    let on_ready = app.on_ready.take();
    // From here, so a link the scene is handed before the view has a size waits among the
    // app's updates and runs before its first frame.
    let open_urls = crate::open_url::incoming::listen(&app);
    // A view is one app's one surface: the other windows and the overlays it asked for go
    // with the `App`.
    let mut dropped = Reported::default();
    if !app.windows.is_empty() { report(&mut dropped, Kind::Windows); }
    if !app.overlays.is_empty() { report(&mut dropped, Kind::Overlays); }
    let (on_quit, on_rest) = (app.on_quit, app.on_rest);
    let (spec, state, shared, config) = app.into_parts(None);
    Box::new(move |layer, size, scale, dark, wake| {
        // SAFETY: `layer` is the `CAMetalLayer` of the view that owns this surface, and the
        // view calls this on the main thread.
        let host = unsafe { LayerHost::from_core_animation_layer(layer, &spec, state, size, scale, dark, wake.clone())? };
        // The view is the app's whole host here: links open in the system, as from a window.
        let mut host = host.with_system_clipboard(wake.clone()).with_popup_menus().with_file_dialogs().with_system_links();
        host.runtime_mut().configure(config);
        // `configure` handed the runtime the app's hooks, as a window's host does. The app is
        // hidden until the view says where its scene is (`Surface::lifecycle`), as a window is
        // until it is shown.
        host.runtime_mut().set_live_visible(false);
        // The handles' requests wake the view, as they wake a window's event loop.
        shared.connect(wake.clone());
        let menus = crate::native_menu::MenuGuard::attach(&shared, false);
        Ok(Box::new(Hosted { host, shared, menus, accessibility: AccessibilityLink::with_wake(wake.clone()), wake, title: spec.title, on_ready, dropped, on_quit, on_rest, lifecycle: lifecycle::State::default(), field: field_touch::State::default(), _open_urls: open_urls }))
    })
}
