//! `RuneView`: the `UIView` an app draws in. Its layer is a `CAMetalLayer`; a `CADisplayLink`
//! draws it while something moves and rests otherwise (`pacing`); its touches go to the
//! runtime. An app that owns the screen puts one in its window (`app`), and a UIKit or SwiftUI
//! app puts one wherever it wants a Rune surface (`rune::ios::view`).

#![allow(unsafe_code)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::NSString;
use objc2::rc::{Retained, Weak};
use objc2::runtime::{AnyClass, AnyObject, NSObject, NSObjectProtocol};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, class, define_class, extern_class, extern_protocol, msg_send, sel};

use super::accessibility::Accessibility;
use super::edit_menu;
use super::keys;
use super::pacing::{self, Effect, Event};
use super::surface::{Mount, Surface};
use super::text::input::RuneTextInput;
use crate::frame::loop_frame::LoopFrame;
use crate::machine::Seq;
use crate::platform::main_queue::after_main;
use crate::platform::objc_boundary::guarded;
use crate::runtime::TouchPhase;

mod accessibility;
mod files;
mod input;
mod lifecycle;
mod link;
mod menu;
mod mirror;
mod overlays;
mod photos;
mod pointer_lock;
mod share;
mod tint;

use link::MainOnly;

extern_protocol!(
    /// A gesture recognizer asks its delegate whether another may recognize alongside it.
    #[name = "UIGestureRecognizerDelegate"]
    unsafe trait GestureRecognizerDelegate: NSObjectProtocol {}
);

extern_protocol!(
    /// A document picker tells its delegate what was picked, or that nothing was.
    #[name = "UIDocumentPickerDelegate"]
    unsafe trait DocumentPickerDelegate: NSObjectProtocol {}
);

extern_protocol!(
    /// A photo picker tells its delegate what was picked, or that nothing was.
    #[name = "PHPickerViewControllerDelegate"]
    unsafe trait PhotoPickerDelegate: NSObjectProtocol {}
);

extern_protocol!(
    /// An edit menu interaction asks its delegate for the menu, its place, and says when it goes.
    #[name = "UIEditMenuInteractionDelegate"]
    unsafe trait EditMenuInteractionDelegate: NSObjectProtocol {}
);

extern_protocol!(
    /// A scroll view asks its delegate whether a tap on the status bar scrolls it to its top.
    #[name = "UIScrollViewDelegate"]
    unsafe trait ScrollViewDelegate: NSObjectProtocol {}
);

extern_class!(
    // SAFETY: UIKit's class of this name is an `NSObject` subclass for the main thread only.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "UIResponder"]
    pub(crate) struct UIResponder;
);

extern_class!(
    // SAFETY: UIKit's class of this name is a `UIResponder` subclass for the main thread only.
    #[unsafe(super(UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "UIView"]
    pub(crate) struct UIView;
);

/// `UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight`.
const FILL_SUPERVIEW: usize = 2 | 16;

/// What the view draws: an app waiting for the view's first size, an app drawing, or nothing
/// because its GPU surface couldn't be made.
enum Stage {
    Waiting(Mount),
    Live(Box<dyn Surface>),
    Failed,
}

pub(crate) struct Ivars {
    stage: RefCell<Stage>,
    pacing: Cell<pacing::State>,
    seq: Cell<Seq>,
    loop_frame: Cell<LoopFrame>,
    link: RefCell<Option<Retained<AnyObject>>>,
    /// A wake from another thread is on its way to the main queue: later ones ride with it.
    wake_queued: Arc<AtomicBool>,
    /// What the keyboard talks to, a hidden view inside this one.
    text: RefCell<Option<Retained<RuneTextInput>>>,
    /// How far the soft keyboard reaches up over the view's bottom edge, in points.
    keyboard: Cell<f32>,
    accessibility: RefCell<Accessibility>,
    /// The file dialog whose picker is up.
    file_dialog: RefCell<Option<files::Picking>>,
    /// The dialog whose photo picker is up.
    media_dialog: Cell<Option<u64>>,
    menu: RefCell<edit_menu::State>,
    menu_seq: Cell<Seq>,
    /// What presents the menu, made when the first one opens.
    edit_menu: RefCell<Option<Retained<AnyObject>>>,
    /// The hardware key that repeats while held.
    keys: RefCell<keys::Repeat>,
    key_seq: Cell<Seq>,
    mirror: RefCell<Option<Retained<mirror::Mirror>>>,
    mirrored_by: RefCell<Weak<AnyObject>>,
    overlays: RefCell<Vec<overlays::Overlay>>,
    control: RefCell<Option<Rc<RefCell<crate::overlay::ios::Control>>>>,
    pointer_lock: Cell<super::pointer_lock::State>,
    pointer_lock_seq: Cell<Seq>,
}

define_class!(
    // SAFETY: `UIView` is made to be subclassed, and `RuneView` has no `Drop`.
    #[unsafe(super(UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RuneView"]
    #[ivars = Ivars]
    pub(crate) struct RuneView;

    // SAFETY: the superclass chain ends in `NSObject`, which implements the protocol.
    unsafe impl NSObjectProtocol for RuneView {}

    // SAFETY: every method of the protocol is optional.
    unsafe impl GestureRecognizerDelegate for RuneView {
        /// A pinch and a turn are one two-finger gesture: both run at once, as on a trackpad.
        #[unsafe(method(gestureRecognizer:shouldRecognizeSimultaneouslyWithGestureRecognizer:))]
        fn recognizes_with(&self, _recognizer: &AnyObject, _other: &AnyObject) -> bool {
            guarded("gestureRecognizer:shouldRecognizeSimultaneouslyWithGestureRecognizer:", || true)
        }

        #[unsafe(method(gestureRecognizer:shouldReceiveTouch:))]
        fn receives_touch(&self, _recognizer: &AnyObject, touch: &AnyObject) -> bool {
            guarded("gestureRecognizer:shouldReceiveTouch:", || {
                // SAFETY: a touch's `view` is a nullable `UIView`; `isKindOfClass:` answers a `BOOL`.
                unsafe {
                    let view: Option<Retained<AnyObject>> = msg_send![touch, view];
                    view.is_none_or(|view| {
                        if crate::native_view::ios::hosted(&view) { return false; }
                        let rune: bool = msg_send![&*view, isKindOfClass: class!(RuneView)];
                        !rune || std::ptr::eq(Retained::as_ptr(&view).cast::<RuneView>(), self)
                    })
                }
            })
        }

        /// A long press begins only over something with a menu to open.
        #[unsafe(method(gestureRecognizerShouldBegin:))]
        fn recognizer_begins(&self, recognizer: &AnyObject) -> bool {
            guarded("gestureRecognizerShouldBegin:", || {
                // SAFETY: `isKindOfClass:` takes a class and answers a `BOOL`.
                let long_press: bool = unsafe { msg_send![recognizer, isKindOfClass: class!(UILongPressGestureRecognizer)] };
                !long_press || self.long_press_begins(recognizer)
            })
        }
    }

    // SAFETY: every method of the protocol is optional.
    unsafe impl DocumentPickerDelegate for RuneView {
        #[unsafe(method(documentPicker:didPickDocumentsAtURLs:))]
        fn document_picked(&self, _picker: &AnyObject, urls: &AnyObject) {
            guarded("documentPicker:didPickDocumentsAtURLs:", || { self.file_picked(Some(urls)); })
        }

        #[unsafe(method(documentPickerWasCancelled:))]
        fn document_picker_cancelled(&self, _picker: &AnyObject) {
            guarded("documentPickerWasCancelled:", || { self.file_picked(None); })
        }
    }

    // SAFETY: the protocol's one method is implemented with its signature.
    unsafe impl PhotoPickerDelegate for RuneView {
        #[unsafe(method(picker:didFinishPicking:))]
        fn photos_picked(&self, picker: &AnyObject, results: &AnyObject) {
            guarded("picker:didFinishPicking:", || { self.media_picked(picker, results); })
        }
    }

    // SAFETY: every method of the protocol is optional.
    unsafe impl EditMenuInteractionDelegate for RuneView {
        #[unsafe(method_id(editMenuInteraction:menuForConfiguration:suggestedActions:))]
        fn edit_menu(&self, _interaction: &AnyObject, _configuration: &AnyObject, _suggested: &AnyObject) -> Option<Retained<AnyObject>> {
            guarded("editMenuInteraction:menuForConfiguration:suggestedActions:", || self.menu_items())
        }

        #[unsafe(method(editMenuInteraction:targetRectForConfiguration:))]
        fn edit_menu_target(&self, _interaction: &AnyObject, _configuration: &AnyObject) -> CGRect {
            guarded("editMenuInteraction:targetRectForConfiguration:", || self.menu_target())
        }

        #[unsafe(method(editMenuInteraction:willDismissMenuForConfiguration:animator:))]
        fn edit_menu_dismissed(&self, _interaction: &AnyObject, _configuration: &AnyObject, _animator: &AnyObject) {
            guarded("editMenuInteraction:willDismissMenuForConfiguration:animator:", || { self.menu_dismissed(); })
        }
    }

    // SAFETY: every method of the protocol is optional.
    unsafe impl ScrollViewDelegate for RuneView {
        /// The mirror stays put: the runtime scrolls the app's own list.
        #[unsafe(method(scrollViewShouldScrollToTop:))]
        fn should_scroll_to_top(&self, _mirror: &AnyObject) -> bool {
            guarded("scrollViewShouldScrollToTop:", || {
                self.status_bar_tapped();
                false
            })
        }
    }

    impl RuneView {
        #[unsafe(method(layerClass))]
        fn layer_class() -> &'static AnyClass {
            guarded("layerClass", || class!(CAMetalLayer))
        }

        #[unsafe(method_id(hitTest:withEvent:))]
        fn hit_test(&self, point: CGPoint, event: Option<&AnyObject>) -> Option<Retained<AnyObject>> {
            guarded("hitTest:withEvent:", || {
                // SAFETY: `UIView` implements it with this signature, answering a nullable view that
                // its superview keeps alive, which is retained here.
                let hit: Option<Retained<AnyObject>> = unsafe {
                    let hit: *mut AnyObject = msg_send![super(self), hitTest: point, withEvent: event];
                    Retained::retain(hit)
                };
                let on_self = hit.as_ref().is_some_and(|hit| std::ptr::eq(Retained::as_ptr(hit).cast::<RuneView>(), self));
                hit.filter(|_| !on_self || self.overlay_takes(point.x as f32, point.y as f32) != Some(false))
            })
        }

        #[unsafe(method(layoutSubviews))]
        fn layout_subviews(&self) {
            guarded("layoutSubviews", || {
                // SAFETY: `UIView` implements it with this signature.
                let _: () = unsafe { msg_send![super(self), layoutSubviews] };
                self.laid_out();
            })
        }

        #[unsafe(method(didMoveToWindow))]
        fn did_move_to_window(&self) {
            guarded("didMoveToWindow", || {
                // SAFETY: `UIView` implements it with this signature.
                let _: () = unsafe { msg_send![super(self), didMoveToWindow] };
                self.moved_to_window();
                self.offer_mirror();
            })
        }

        #[unsafe(method(didMoveToSuperview))]
        fn did_move_to_superview(&self) {
            guarded("didMoveToSuperview", || {
                // SAFETY: `UIView` implements it with this signature.
                let _: () = unsafe { msg_send![super(self), didMoveToSuperview] };
                self.offer_mirror();
            })
        }

        #[unsafe(method(traitCollectionDidChange:))]
        fn trait_collection_did_change(&self, previous: Option<&AnyObject>) {
            guarded("traitCollectionDidChange:", || {
                // SAFETY: `UIView` implements it with this signature.
                let _: () = unsafe { msg_send![super(self), traitCollectionDidChange: previous] };
                self.traits_changed();
            })
        }

        #[unsafe(method(tintColorDidChange))]
        fn tint_color_did_change(&self) {
            guarded("tintColorDidChange", || {
                // SAFETY: `UIView` implements it with this signature.
                let _: () = unsafe { msg_send![super(self), tintColorDidChange] };
                self.tint_changed();
            })
        }

        #[unsafe(method(safeAreaInsetsDidChange))]
        fn safe_area_insets_did_change(&self) {
            guarded("safeAreaInsetsDidChange", || {
                // SAFETY: `UIView` implements it with this signature.
                let _: () = unsafe { msg_send![super(self), safeAreaInsetsDidChange] };
                self.safe_area_changed();
            })
        }

        #[unsafe(method(touchesBegan:withEvent:))]
        fn touches_began(&self, touches: &AnyObject, event: Option<&AnyObject>) {
            guarded("touchesBegan:withEvent:", || if !self.secondary_click(touches, event) { self.touches(touches, TouchPhase::Started); })
        }

        #[unsafe(method(touchesMoved:withEvent:))]
        fn touches_moved(&self, touches: &AnyObject, _event: Option<&AnyObject>) {
            guarded("touchesMoved:withEvent:", || { self.touches(touches, TouchPhase::Moved); })
        }

        #[unsafe(method(touchesEnded:withEvent:))]
        fn touches_ended(&self, touches: &AnyObject, _event: Option<&AnyObject>) {
            guarded("touchesEnded:withEvent:", || { self.touches(touches, TouchPhase::Ended); })
        }

        #[unsafe(method(touchesCancelled:withEvent:))]
        fn touches_cancelled(&self, touches: &AnyObject, _event: Option<&AnyObject>) {
            guarded("touchesCancelled:withEvent:", || { self.touches(touches, TouchPhase::Cancelled); })
        }

        #[unsafe(method(runeHover:))]
        fn hover(&self, recognizer: &AnyObject) {
            guarded("runeHover:", || { self.hovered(recognizer); })
        }

        #[unsafe(method(runeScroll:))]
        fn scroll(&self, recognizer: &AnyObject) {
            guarded("runeScroll:", || { self.scrolled(recognizer); })
        }

        #[unsafe(method(runeLongPress:))]
        fn long_press(&self, recognizer: &AnyObject) {
            guarded("runeLongPress:", || { self.long_pressed(recognizer); })
        }

        /// An item of the app's own menus (`App::menu_item`), picked from the menu bar or by
        /// its shortcut: the command carries the item's id.
        #[unsafe(method(runeMenuItem:))]
        fn menu_item(&self, command: &AnyObject) {
            guarded("runeMenuItem:", || {
                // SAFETY: the commands `native_menu` makes carry an `NSString`.
                let id: Option<Retained<NSString>> = unsafe { msg_send![command, propertyList] };
                if let Some(id) = id && crate::native_menu::perform_ios_item(&id.to_string()) { self.pace(Event::Wake); }
            })
        }

        #[unsafe(method(runeMenuPick:))]
        fn menu_pick(&self, command: &AnyObject) {
            guarded("runeMenuPick:", || { self.menu_picked(command); })
        }

        #[unsafe(method(runePinch:))]
        fn pinch(&self, recognizer: &AnyObject) {
            guarded("runePinch:", || { self.pinched(recognizer); })
        }

        #[unsafe(method(runeRotate:))]
        fn rotate(&self, recognizer: &AnyObject) {
            guarded("runeRotate:", || { self.rotated(recognizer); })
        }

        #[unsafe(method(runeTick:))]
        fn tick(&self, _link: &AnyObject) {
            guarded("runeTick:", || { self.pace(Event::Tick); })
        }

        #[unsafe(method(runeKeyboard:))]
        fn keyboard(&self, notification: &AnyObject) {
            guarded("runeKeyboard:", || { self.keyboard_moved(notification); })
        }

        #[unsafe(method(runeTextSize:))]
        fn text_size(&self, _notification: &AnyObject) {
            guarded("runeTextSize:", || { self.text_size_changed(); })
        }

        #[unsafe(method(runeDisplayOptions:))]
        fn display_options(&self, _notification: &AnyObject) {
            guarded("runeDisplayOptions:", || { self.display_options_changed(); })
        }

        #[unsafe(method(runeReduceMotion:))]
        fn reduce_motion(&self, _notification: &AnyObject) {
            guarded("runeReduceMotion:", || { self.reduce_motion_changed(); })
        }

        #[unsafe(method(runeMemoryWarning:))]
        fn memory_warning(&self, _notification: &AnyObject) {
            guarded("runeMemoryWarning:", || { self.memory_warned(); })
        }

        #[unsafe(method(runeForeground:))]
        fn foreground(&self, _notification: &AnyObject) {
            guarded("runeForeground:", || { self.entered_foreground(); })
        }

        #[unsafe(method(runeBackground:))]
        fn background(&self, _notification: &AnyObject) {
            guarded("runeBackground:", || { self.entered_background(); })
        }

        #[unsafe(method(runeActive:))]
        fn active(&self, _notification: &AnyObject) {
            guarded("runeActive:", || { self.became_active(); })
        }

        #[unsafe(method(runeInactive:))]
        fn inactive(&self, _notification: &AnyObject) {
            guarded("runeInactive:", || { self.resigning_active(); })
        }

        #[unsafe(method(runePointerLock:))]
        fn pointer_lock_state(&self, _notification: &AnyObject) {
            guarded("runePointerLock:", || { self.pointer_lock_changed(); })
        }

        #[unsafe(method(runeMouse:))]
        fn mouse(&self, _notification: &AnyObject) {
            guarded("runeMouse:", || { self.mouse_connected(); })
        }

        #[unsafe(method(runeTerminate:))]
        fn terminate(&self, _notification: &AnyObject) {
            guarded("runeTerminate:", || { self.terminating(); })
        }

        /// The view is a container: a reader lands on its elements, never on it.
        #[unsafe(method(isAccessibilityElement))]
        fn is_accessibility_element(&self) -> bool {
            guarded("isAccessibilityElement", || false)
        }

        #[unsafe(method_id(accessibilityElements))]
        fn accessibility_elements(&self) -> Option<Retained<AnyObject>> {
            guarded("accessibilityElements", || Some(self.overlay_elements(self.accessibility_asked())))
        }

        /// With no text field focused the view itself hears a hardware keyboard.
        #[unsafe(method(canBecomeFirstResponder))]
        fn can_become_first_responder(&self) -> bool {
            guarded("canBecomeFirstResponder", || true)
        }

        #[unsafe(method(pressesBegan:withEvent:))]
        fn presses_began(&self, presses: &AnyObject, event: Option<&AnyObject>) {
            guarded("pressesBegan:withEvent:", || {
                if !self.presses(presses, true, false) {
                    // SAFETY: `presses` and `event` are UIKit's own objects, passed unchanged to
                    // `UIView`'s method of the same signature.
                    let _: () = unsafe { msg_send![super(self), pressesBegan: presses, withEvent: event] };
                }
            })
        }

        #[unsafe(method(pressesEnded:withEvent:))]
        fn presses_ended(&self, presses: &AnyObject, event: Option<&AnyObject>) {
            guarded("pressesEnded:withEvent:", || {
                self.presses(presses, false, false);
                // SAFETY: as in `pressesBegan:withEvent:`.
                let _: () = unsafe { msg_send![super(self), pressesEnded: presses, withEvent: event] };
            })
        }

        #[unsafe(method(pressesCancelled:withEvent:))]
        fn presses_cancelled(&self, presses: &AnyObject, event: Option<&AnyObject>) {
            guarded("pressesCancelled:withEvent:", || {
                self.presses(presses, false, false);
                // SAFETY: as in `pressesBegan:withEvent:`.
                let _: () = unsafe { msg_send![super(self), pressesCancelled: presses, withEvent: event] };
            })
        }
    }
);

impl RuneView {
    /// A view that starts `mount`'s app when it first has a size, filling whatever it is put in.
    pub(crate) fn new(mtm: MainThreadMarker, mount: Mount) -> Retained<Self> {
        crate::embed::prewarm();
        let this = Self::alloc(mtm).set_ivars(Ivars {
            stage: RefCell::new(Stage::Waiting(mount)),
            pacing: Cell::new(pacing::State::Detached),
            seq: Cell::new(Seq::default()),
            loop_frame: Cell::new(LoopFrame::default()),
            link: RefCell::new(None),
            wake_queued: Arc::new(AtomicBool::new(false)),
            text: RefCell::new(None),
            keyboard: Cell::new(0.0),
            accessibility: RefCell::new(Accessibility::new()),
            file_dialog: RefCell::new(None),
            media_dialog: Cell::new(None),
            menu: RefCell::default(),
            menu_seq: Cell::new(Seq::default()),
            edit_menu: RefCell::new(None),
            keys: RefCell::new(keys::Repeat::Idle),
            key_seq: Cell::new(Seq::default()),
            mirror: RefCell::new(None),
            mirrored_by: RefCell::default(),
            overlays: RefCell::default(),
            control: RefCell::new(None),
            pointer_lock: Cell::new(super::pointer_lock::State::Free),
            pointer_lock_seq: Cell::new(Seq::default()),
        });
        let frame = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(0.0, 0.0));
        // SAFETY: `initWithFrame:` is `UIView`'s designated initializer.
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        // SAFETY: a `UIView` property that takes a `UIViewAutoresizing` mask.
        let _: () = unsafe { msg_send![&*view, setAutoresizingMask: FILL_SUPERVIEW] };
        let text = RuneTextInput::new(mtm, &view);
        // SAFETY: `addSubview:` takes a view, which the text input is.
        let _: () = unsafe { msg_send![&*view, addSubview: &*text] };
        *view.ivars().text.borrow_mut() = Some(text);
        view.mirror_main_scroll(mtm);
        view.recognize(class!(UIPinchGestureRecognizer), sel!(runePinch:));
        view.recognize(class!(UIRotationGestureRecognizer), sel!(runeRotate:));
        view.recognize(class!(UILongPressGestureRecognizer), sel!(runeLongPress:));
        view.recognize_pointer();
        view
    }

    fn overlay(mtm: MainThreadMarker, mount: Mount, control: Rc<RefCell<crate::overlay::ios::Control>>) -> Retained<Self> {
        let view = Self::new(mtm, mount);
        *view.ivars().control.borrow_mut() = Some(control);
        // SAFETY: `UIView` properties of these types; `clearColor` is a shared `UIColor`.
        unsafe {
            let _: () = msg_send![&*view, setAutoresizingMask: 0usize];
            let _: () = msg_send![&*view, setOpaque: false];
            let clear: Retained<AnyObject> = msg_send![class!(UIColor), clearColor];
            let _: () = msg_send![&*view, setBackgroundColor: &*clear];
        }
        view
    }

    pub(super) fn with_surface<R>(&self, f: impl FnOnce(&mut dyn Surface) -> R) -> Option<R> {
        match &mut *self.ivars().stage.borrow_mut() {
            Stage::Live(surface) => Some(super::surface::in_turn(surface.as_mut(), f)),
            Stage::Waiting(_) | Stage::Failed => None,
        }
    }

    /// Draws the frame the display link's next refresh shows, and tells the pacing how it went.
    fn draw(&self) {
        let Some(target) = self.frame_target() else { return };
        let refresh_hz = self.frame_rate();
        let mut loops = self.ivars().loop_frame.get();
        let only_loops = loops.only_loops();
        self.ivars().loop_frame.set(loops);
        let drawn = self.with_surface(|surface| (surface.frame(target, refresh_hz, only_loops), surface.text(), surface.accessibility(), surface.popup(), surface.file_dialog(), surface.share(), surface.interface_style()));
        let Some((frame, text, tree, popup, file, share, style)) = drawn else {
            self.pace(Event::Frame { animating: false, wake_after: None });
            return;
        };
        // The frame applied the input before it: the keyboard hears what the field holds now.
        let input = self.ivars().text.borrow().clone();
        if let Some(input) = input { input.sync(text.wanted, text.snapshot, text.caret, text.secure, text.traits, text.history); }
        match popup {
            Some((popup, field)) => self.open_menu(popup, field),
            None => self.menu_follow_caret(),
        }
        if let Some(style) = style { self.override_style(style); }
        match file {
            Some((id, _, _, _, Some(media))) => self.pick_media(id, &media),
            Some((id, kind, multiple, options, None)) => self.pick_file(id, kind, multiple, &options),
            None => {}
        }
        if let Some(share) = share { self.show_share(share); }
        if let Some(reading) = tree {
            let posts = self.ivars().accessibility.borrow_mut().apply(self.mtm(), self, reading);
            posts.post();
        }
        self.sync_mirror();
        self.sync_pointer_lock();
        self.pace(Event::Frame { animating: frame.animating, wake_after: frame.wake_after_secs });
    }

    pub(super) fn touched(&self) {
        let mut loops = self.ivars().loop_frame.get();
        loops.touched();
        self.ivars().loop_frame.set(loops);
    }

    fn pace(&self, event: Event) {
        let ivars = self.ivars();
        if event == Event::Wake && ivars.pacing.get() == pacing::State::Detached && let Some(control) = ivars.control.borrow().as_ref() {
            control.borrow().woke();
        }
        if !matches!(event, Event::Tick | Event::Timer(_) | Event::Frame { .. }) { self.touched(); }
        let (mut seq, mut state) = (ivars.seq.get(), ivars.pacing.get());
        let effects = pacing::step(&mut seq, &mut state, event);
        ivars.seq.set(seq);
        ivars.pacing.set(state);
        for effect in effects {
            match effect {
                Effect::Create => self.create_link(),
                Effect::Destroy => self.destroy_link(),
                Effect::Draw => self.draw(),
                Effect::Resume => self.set_paused(false),
                Effect::Pause => self.set_paused(true),
                Effect::TimerAfter(attempt, seconds) => {
                    let view = MainOnly(Weak::new(self));
                    after_main(seconds, move || view.pace(Event::Timer(attempt)));
                }
            }
        }
    }
}
