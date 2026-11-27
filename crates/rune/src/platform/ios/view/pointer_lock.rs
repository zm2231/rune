#![allow(unsafe_code)]

use std::sync::{Once, OnceLock};

use block2::{Block, RcBlock};
use objc2_foundation::NSString;
use objc2::rc::{Retained, Weak};
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, class, msg_send, sel};

use super::super::controller::RuneViewController;
use super::super::pacing::Event as Pace;
use super::super::pointer_lock::{self, Effect, Event};
use super::{MainOnly, RuneView};
use crate::machine::Attempt;
use crate::platform::main_queue::after_main;
use crate::platform::objc_boundary::guarded;
use crate::runtime::LockEffect;

type Moved = Block<dyn Fn(*mut AnyObject, f32, f32)>;

// SAFETY: UIKit's and GameController's headers declare each as an `NSNotificationName`, an
// object that lives for the process. Linking GameController brings in `GCMouse`, which
// `mouse_motion` looks up by name.
#[link(name = "UIKit", kind = "framework")]
unsafe extern "C" {
    static UIPointerLockStateDidChangeNotification: &'static AnyObject;
}
#[link(name = "GameController", kind = "framework")]
unsafe extern "C" {
    static GCMouseDidConnectNotification: &'static AnyObject;
}

impl RuneView {
    pub(super) fn observe_pointer_lock(&self, centre: &AnyObject) {
        // SAFETY: each selector is a method of this class that takes the notification, and the
        // caller removes the view as an observer before it goes.
        unsafe {
            let _: () = msg_send![centre, addObserver: self, selector: sel!(runePointerLock:), name: UIPointerLockStateDidChangeNotification, object: None::<&AnyObject>];
            let _: () = msg_send![centre, addObserver: self, selector: sel!(runeMouse:), name: GCMouseDidConnectNotification, object: None::<&AnyObject>];
        }
        self.offer_pointer_lock();
    }

    pub(super) fn offer_pointer_lock(&self) {
        let held = self.rune_controller().is_some();
        self.with_surface(|surface| surface.set_pointer_lock(held));
        self.sync_pointer_lock();
    }

    pub(super) fn sync_pointer_lock(&self) {
        let Some(effect) = self.with_surface(|surface| surface.take_pointer_lock()).flatten() else { return };
        self.pointer_lock(match effect {
            LockEffect::Grab => Event::Grab,
            LockEffect::Ungrab => Event::Ungrab,
        });
    }

    pub(super) fn finger_pressed(&self) {
        static TOLD: Once = Once::new();
        if TOLD.is_completed() || !self.with_surface(|surface| surface.pointer_lock_ready()).unwrap_or(false) || pointer_clicks_are_pointers() { return; }
        TOLD.call_once(|| eprintln!("rune: El::pointer_lock can't capture on this iPad: the app's Info.plist doesn't set UIApplicationSupportsIndirectInputEvents (iPadOS before 17 reads a missing key as NO), so mouse and trackpad clicks arrive as finger touches"));
    }

    pub(super) fn pointer_lock_changed(&self) {
        let locked = self.scene_locks_pointer();
        self.pointer_lock(Event::Changed { locked });
    }

    pub(super) fn mouse_connected(&self) {
        if self.ivars().pointer_lock.get() == pointer_lock::State::Held { self.mouse_motion(true); }
    }

    fn pointer_lock_due(&self, attempt: Attempt) {
        let locked = self.scene_locks_pointer();
        self.pointer_lock(Event::Due { attempt, locked });
    }

    fn pointer_lock(&self, event: Event) {
        let ivars = self.ivars();
        let (mut seq, mut state) = (ivars.pointer_lock_seq.get(), ivars.pointer_lock.get());
        let effects = pointer_lock::step(&mut seq, &mut state, event);
        ivars.pointer_lock_seq.set(seq);
        ivars.pointer_lock.set(state);
        for effect in effects {
            match effect {
                Effect::Prefer(locked) => if let Some(controller) = self.rune_controller() { controller.prefer_pointer_locked(locked) },
                Effect::Motion(on) => self.mouse_motion(on),
                Effect::Granted | Effect::Lost => {
                    self.with_surface(|surface| surface.pointer_lock_answer(effect == Effect::Granted));
                    self.pace(Pace::Wake);
                    self.sync_pointer_lock();
                }
                Effect::AnswerAfter(attempt, seconds) => {
                    let view = MainOnly(Weak::new(self));
                    after_main(seconds, move || view.pointer_lock_due(attempt));
                }
            }
        }
    }

    fn rune_controller(&self) -> Option<Retained<RuneViewController>> {
        // SAFETY: `nextResponder` is a nullable `UIResponder`, here checked to be a
        // `RuneViewController` before it is cast to one.
        unsafe {
            let next: Option<Retained<AnyObject>> = msg_send![self, nextResponder];
            next.filter(|next| RuneViewController::owns(Some(next))).map(|next| Retained::cast_unchecked(next))
        }
    }

    fn scene_locks_pointer(&self) -> bool {
        // SAFETY: `UIView.window`, `UIWindow.windowScene` and `UIWindowScene.pointerLockState`
        // are nullable objects, and `UIPointerLockState.isLocked` is a `BOOL`.
        unsafe {
            let window: Option<Retained<AnyObject>> = msg_send![self, window];
            let Some(window) = window else { return false };
            let scene: Option<Retained<AnyObject>> = msg_send![&*window, windowScene];
            let Some(scene) = scene else { return false };
            let state: Option<Retained<AnyObject>> = msg_send![&*scene, pointerLockState];
            state.is_some_and(|state| msg_send![&*state, isLocked])
        }
    }

    fn mouse_motion(&self, on: bool) {
        let handler = on.then(|| {
            let view = Weak::new(self);
            RcBlock::new(move |_input: *mut AnyObject, dx: f32, dy: f32| {
                guarded("mouseMovedHandler", || {
                    if MainThreadMarker::new().is_none() { return; }
                    let Some(view) = view.load() else { return };
                    view.with_surface(|surface| surface.pointer_motion(dx, -dy));
                    view.pace(Pace::Wake);
                })
            })
        });
        // SAFETY: `GCMouse.mice` is an array of `GCMouse`, whose `mouseInput` is a nullable
        // `GCMouseInput`; its `mouseMovedHandler` takes a nullable block of this signature,
        // which it copies and calls on the mouse's handler queue, the main queue unless an
        // app moved it, which the block checks.
        unsafe {
            let mice: Retained<AnyObject> = msg_send![class!(GCMouse), mice];
            let count: usize = msg_send![&*mice, count];
            for index in 0..count {
                let mouse: Retained<AnyObject> = msg_send![&*mice, objectAtIndex: index];
                let input: Option<Retained<AnyObject>> = msg_send![&*mouse, mouseInput];
                let Some(input) = input else { continue };
                let _: () = msg_send![&*input, setMouseMovedHandler: handler.as_deref().map(|block| -> &Moved { block })];
            }
        }
    }
}

impl MainOnly {
    fn pointer_lock_due(&self, attempt: Attempt) {
        if let Some(view) = self.0.load() { view.pointer_lock_due(attempt); }
    }
}

fn pointer_clicks_are_pointers() -> bool {
    static POINTERS: OnceLock<bool> = OnceLock::new();
    *POINTERS.get_or_init(|| {
        // SAFETY: `UIDevice.currentDevice` and `NSBundle.mainBundle` are singletons read on the
        // main thread; `userInterfaceIdiom` is an `NSInteger` enum, `systemVersion` a string,
        // and an Info.plist value is a nullable object, a number for a Boolean key.
        unsafe {
            let device: Retained<AnyObject> = msg_send![class!(UIDevice), currentDevice];
            let idiom: isize = msg_send![&*device, userInterfaceIdiom];
            if idiom != IDIOM_PAD { return true; }
            let bundle: Retained<AnyObject> = msg_send![class!(NSBundle), mainBundle];
            let key = NSString::from_str("UIApplicationSupportsIndirectInputEvents");
            let value: Option<Retained<AnyObject>> = msg_send![&*bundle, objectForInfoDictionaryKey: &*key];
            if let Some(value) = value { return msg_send![&*value, boolValue]; }
            let version: Retained<NSString> = msg_send![&*device, systemVersion];
            version.to_string().split('.').next().and_then(|major| major.parse::<u32>().ok()).is_some_and(|major| major >= 17)
        }
    })
}

const IDIOM_PAD: isize = 1;
