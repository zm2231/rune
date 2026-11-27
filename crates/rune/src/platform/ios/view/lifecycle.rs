//! A `RuneView` joining and leaving a window, and what the system tells it while it is in one.
//! The view's first size starts its app on the view's layer. From then on its size, the safe
//! area, the soft keyboard's frame, the appearance, the tint, the text size and Reduce Motion
//! settings, and its scene's phases (foreground and background, active and inactive) each
//! reach the surface and wake the pacing.
//!
//! The phases are the view's own scene's, from `UIScene`'s notifications for that scene: on an
//! iPad an app's scenes come forward, lose the focus and go away one by one, and
//! `UIApplication`'s notifications speak for all of them at once. A window with no scene
//! hears `UIApplication`'s instead. Termination is the app's alone.
//!
//! A memory warning reaches the surface in or out of a window. It wakes the pacing only when
//! the app's `App::on_rest` left a frame work to do.

#![allow(unsafe_code)]

use std::ffi::c_void;

use objc2_core_foundation::{CGFloat, CGRect};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, Encode, Encoding, class, msg_send, sel};

use super::super::accessibility::Accessibility;
use super::super::controller::RuneViewController;
use super::super::keys;
use super::super::lifecycle::{self, Phase};
use super::super::pacing::Event;
use super::{RuneView, Stage};
use crate::platform::preferences::{bold_text, contrast, increased_contrast_with, reduced_motion, text_scale};

// SAFETY: UIKit's headers declare each as an `NSNotificationName` or an `NSString`, an object
// that lives for the process.
#[link(name = "UIKit", kind = "framework")]
unsafe extern "C" {
    static UIApplicationDidEnterBackgroundNotification: &'static AnyObject;
    static UIApplicationWillEnterForegroundNotification: &'static AnyObject;
    static UIApplicationDidBecomeActiveNotification: &'static AnyObject;
    static UIApplicationWillResignActiveNotification: &'static AnyObject;
    static UIApplicationWillTerminateNotification: &'static AnyObject;
    static UISceneDidEnterBackgroundNotification: &'static AnyObject;
    static UISceneWillEnterForegroundNotification: &'static AnyObject;
    static UISceneDidActivateNotification: &'static AnyObject;
    static UISceneWillDeactivateNotification: &'static AnyObject;
    static UIApplicationDidReceiveMemoryWarningNotification: &'static AnyObject;
    static UIKeyboardWillChangeFrameNotification: &'static AnyObject;
    static UIKeyboardFrameEndUserInfoKey: &'static AnyObject;
    static UIAccessibilityReduceMotionStatusDidChangeNotification: &'static AnyObject;
    static UIContentSizeCategoryDidChangeNotification: &'static AnyObject;
    static UIAccessibilityBoldTextStatusDidChangeNotification: &'static AnyObject;
    static UIAccessibilityDarkerSystemColorsStatusDidChangeNotification: &'static AnyObject;
}

/// `UIUserInterfaceStyleLight` and `UIUserInterfaceStyleDark`.
pub(super) const STYLE_LIGHT: isize = 1;
pub(super) const STYLE_DARK: isize = 2;
/// `UIEdgeInsets`.
#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct EdgeInsets {
    pub(super) top: CGFloat,
    pub(super) left: CGFloat,
    pub(super) bottom: CGFloat,
    pub(super) right: CGFloat,
}

// SAFETY: `UIEdgeInsets` is four `CGFloat`s in this order.
unsafe impl Encode for EdgeInsets {
    const ENCODING: Encoding = Encoding::Struct("UIEdgeInsets", &[CGFloat::ENCODING, CGFloat::ENCODING, CGFloat::ENCODING, CGFloat::ENCODING]);
}

impl RuneView {
    /// The view's size in points and its scale.
    pub(super) fn metrics(&self) -> ((f32, f32), f32) {
        // SAFETY: `UIView` properties of these types.
        let (bounds, scale): (CGRect, CGFloat) = unsafe { (msg_send![self, bounds], msg_send![self, contentScaleFactor]) };
        ((bounds.size.width as f32, bounds.size.height as f32), scale as f32)
    }

    pub(super) fn override_style(&self, style: isize) {
        // SAFETY: `overrideUserInterfaceStyle` is a `UIView` property of the `NSInteger` enum
        // `UIUserInterfaceStyle`, set on the main thread.
        unsafe { let _: () = msg_send![self, setOverrideUserInterfaceStyle: style]; }
        // SAFETY: `nextResponder` is a nullable `UIResponder`; a `RuneViewController` is a
        // `UIViewController`, whose `overrideUserInterfaceStyle` takes the same enum.
        unsafe {
            let next: Option<Retained<AnyObject>> = msg_send![self, nextResponder];
            if let Some(controller) = next.filter(|next| RuneViewController::owns(Some(next))) {
                let _: () = msg_send![&*controller, setOverrideUserInterfaceStyle: style];
            }
        }
    }

    fn dark(&self) -> bool {
        // SAFETY: `traitCollection` is never nil, and `userInterfaceStyle` is an `NSInteger` enum.
        let style: isize = unsafe {
            let traits: Retained<AnyObject> = msg_send![self, traitCollection];
            msg_send![&*traits, userInterfaceStyle]
        };
        style == STYLE_DARK
    }

    fn size_classes(&self) -> crate::SizeClasses {
        // SAFETY: `traitCollection` is never nil, and its size classes are `NSInteger` enums.
        let (horizontal, vertical): (isize, isize) = unsafe {
            let traits: Retained<AnyObject> = msg_send![self, traitCollection];
            (msg_send![&*traits, horizontalSizeClass], msg_send![&*traits, verticalSizeClass])
        };
        super::super::size_class::classes(horizontal, vertical)
    }

    /// What the screen keeps at the view's top, right, bottom and left edges: the status bar,
    /// the home indicator, the display's corners where the view reaches them, and the soft
    /// keyboard while it shows.
    pub(super) fn safe_area(&self) -> [f32; 4] {
        // SAFETY: a `UIView` property of type `UIEdgeInsets`.
        let insets: EdgeInsets = unsafe { msg_send![self, safeAreaInsets] };
        // The soft keyboard takes the bottom of the screen as the home indicator does: content
        // padded by the safe area stays above it.
        let bottom = (insets.bottom as f32).max(self.ivars().keyboard.get());
        [insets.top as f32, insets.right as f32, bottom, insets.left as f32]
    }

    /// The view has a size, or a new one: its first starts the app on the view's layer.
    pub(super) fn laid_out(&self) {
        let (size, scale) = self.metrics();
        if size.0 <= 0.0 || size.1 <= 0.0 { return; }
        // Only the size that starts the app reads the tint; later it arrives with its own
        // change or the traits'. Read before the stage is borrowed for the swap, as the tint's
        // own change borrows it.
        let waiting = matches!(*self.ivars().stage.borrow(), Stage::Waiting(_));
        let accent = waiting.then(|| self.tint());
        let mut overlays = Vec::new();
        let mut stage = self.ivars().stage.borrow_mut();
        *stage = match std::mem::replace(&mut *stage, Stage::Failed) {
            Stage::Waiting(mount) => {
                // SAFETY: the view's layer is the `CAMetalLayer` `layerClass` names, and the
                // view keeps it for as long as it lives.
                let layer: Retained<AnyObject> = unsafe { msg_send![self, layer] };
                match mount(Retained::as_ptr(&layer) as *mut c_void, size, scale, self.dark(), self.waker()) {
                    Ok(mut surface) => {
                        surface.enter_turn();
                        // The app hears where the scene of the view's window is.
                        if let Some((_, phase)) = self.scene() { surface.lifecycle(lifecycle::Event::Attached(phase)); }
                        surface.set_safe_area(self.safe_area());
                        if let Some(accent) = accent { surface.set_accent(accent); }
                        surface.set_reduced_motion(reduced_motion());
                        surface.set_text_scale(text_scale());
                        surface.set_bold_text(bold_text());
                        surface.set_increased_contrast(increased_contrast_with(contrast().is_some()));
                        surface.set_size_classes(self.size_classes());
                        // A client that asked before the app started still gets its tree.
                        if self.ivars().accessibility.borrow().asked() { let _ = surface.accessibility_wanted(); }
                        overlays = surface.take_overlays();
                        surface.leave_turn();
                        Stage::Live(surface)
                    }
                    Err(error) => {
                        eprintln!("rune: this view draws nothing: {error}");
                        Stage::Failed
                    }
                }
            }
            Stage::Live(mut surface) => {
                super::super::surface::in_turn(surface.as_mut(), |surface| surface.resize(size, scale));
                Stage::Live(surface)
            }
            Stage::Failed => Stage::Failed,
        };
        drop(stage);
        self.adopt_overlays(overlays);
        self.place_overlays();
        self.pace(Event::Wake);
    }

    /// The scene of the view's window, when the window has one, and the phase it is in; none
    /// for a view in no window.
    fn scene(&self) -> Option<(Option<Retained<AnyObject>>, Phase)> {
        // SAFETY: `UIView.window` and `UIWindow.windowScene` are nullable objects, a scene's
        // `activationState` and the shared application's `applicationState` are `NSInteger`
        // enums, and the application is a singleton.
        unsafe {
            let window: Option<Retained<AnyObject>> = msg_send![self, window];
            let scene: Option<Retained<AnyObject>> = msg_send![&*window?, windowScene];
            let scene_state: Option<isize> = scene.as_deref().map(|scene| msg_send![scene, activationState]);
            let app: Retained<AnyObject> = msg_send![class!(UIApplication), sharedApplication];
            let app_state: isize = msg_send![&*app, applicationState];
            Some((scene, Phase::from_raw(scene_state, app_state)))
        }
    }

    /// Tells the surface of a lifecycle event, and stops the key repeat when held keys were
    /// released.
    fn lifecycle(&self, event: lifecycle::Event) {
        let released = self.with_surface(|surface| surface.lifecycle(event));
        // With no app started nothing hears a key, and the repeat stops all the same.
        if released.unwrap_or(true) { self.keys(keys::Event::Reset); }
    }

    /// The view joined a window, or left the one it was in: in a window it draws at the
    /// screen's scale and hears the system's notifications. Out of one it hears only a memory
    /// warning.
    pub(super) fn moved_to_window(&self) {
        // SAFETY: `UIView.window` is a nullable object, a window's screen is never nil and its
        // `nativeScale` is a `CGFloat`, the notification centre is a singleton, each selector
        // is a method of this class that takes the notification, and a notification's object
        // is a nullable object, here the window's scene.
        unsafe {
            let centre: Retained<AnyObject> = msg_send![class!(NSNotificationCenter), defaultCenter];
            let _: () = msg_send![&*centre, removeObserver: self];
            // In a window or out of one, an app that started hears the app end.
            let _: () = msg_send![&*centre, addObserver: self, selector: sel!(runeTerminate:), name: UIApplicationWillTerminateNotification, object: None::<&AnyObject>];
            // A view out of its window keeps its surface and its caches (a screen under a
            // pushed one), so it hears a memory warning there too.
            let _: () = msg_send![&*centre, addObserver: self, selector: sel!(runeMemoryWarning:), name: UIApplicationDidReceiveMemoryWarningNotification, object: None::<&AnyObject>];
            let window: Option<Retained<AnyObject>> = msg_send![self, window];
            let (Some(window), Some((scene, phase))) = (window, self.scene()) else {
                // A view out of its window hears no key-up.
                self.keys(keys::Event::Reset);
                self.lifecycle(lifecycle::Event::Detached);
                self.pace(Event::Detached);
                return;
            };
            // A view on a `CAMetalLayer` starts at one pixel a point, whatever the screen: left
            // there, the layer is drawn at a third of an iPhone's pixels and stretched.
            let screen: Retained<AnyObject> = msg_send![&*window, screen];
            let scale: CGFloat = msg_send![&*screen, nativeScale];
            let _: () = msg_send![self, setContentScaleFactor: scale];
            let _: () = msg_send![self, setNeedsLayout];
            // The scene's own phases, or the app's for a window with no scene.
            let phases = match &scene {
                Some(_) => [UISceneDidEnterBackgroundNotification, UISceneWillEnterForegroundNotification, UISceneDidActivateNotification, UISceneWillDeactivateNotification],
                None => [UIApplicationDidEnterBackgroundNotification, UIApplicationWillEnterForegroundNotification, UIApplicationDidBecomeActiveNotification, UIApplicationWillResignActiveNotification],
            };
            for (name, selector) in phases.into_iter().zip([sel!(runeBackground:), sel!(runeForeground:), sel!(runeActive:), sel!(runeInactive:)]) {
                let _: () = msg_send![&*centre, addObserver: self, selector: selector, name: name, object: scene.as_deref()];
            }
            let _: () = msg_send![&*centre, addObserver: self, selector: sel!(runeKeyboard:), name: UIKeyboardWillChangeFrameNotification, object: None::<&AnyObject>];
            let _: () = msg_send![&*centre, addObserver: self, selector: sel!(runeReduceMotion:), name: UIAccessibilityReduceMotionStatusDidChangeNotification, object: None::<&AnyObject>];
            let _: () = msg_send![&*centre, addObserver: self, selector: sel!(runeTextSize:), name: UIContentSizeCategoryDidChangeNotification, object: None::<&AnyObject>];
            for name in [UIAccessibilityBoldTextStatusDidChangeNotification, UIAccessibilityDarkerSystemColorsStatusDidChangeNotification] {
                let _: () = msg_send![&*centre, addObserver: self, selector: sel!(runeDisplayOptions:), name: name, object: None::<&AnyObject>];
            }
            self.lifecycle(lifecycle::Event::Attached(phase));
            self.pace(Event::Attached { foreground: phase != Phase::Background });
            // A reader already running reads the first frame.
            if Accessibility::reader_running() { self.accessibility_asked(); }
        }
    }

    /// The system's appearance, or another of the view's traits, changed: the tint is resolved
    /// for the new traits.
    pub(super) fn traits_changed(&self) {
        let (dark, accent, classes) = (self.dark(), self.tint(), self.size_classes());
        self.with_surface(|surface| {
            surface.set_dark(dark);
            surface.set_accent(accent);
            surface.set_size_classes(classes);
        });
        self.pace(Event::Wake);
    }

    /// The view's safe area changed.
    pub(super) fn safe_area_changed(&self) {
        let insets = self.safe_area();
        self.with_surface(|surface| surface.set_safe_area(insets));
        self.place_overlays();
        self.pace(Event::Wake);
    }

    /// The soft keyboard is moving to a new frame: what it covers of the view's bottom joins
    /// the safe area.
    pub(super) fn keyboard_moved(&self, notification: &AnyObject) {
        // SAFETY: the notification's `userInfo` holds the keyboard's final frame as an
        // `NSValue` of a `CGRect` in the screen's coordinates, which a nil source view
        // converts from; `bounds` is a `CGRect`.
        let (overlap, height) = unsafe {
            let info: Option<Retained<AnyObject>> = msg_send![notification, userInfo];
            let Some(info) = info else { return };
            let value: Option<Retained<AnyObject>> = msg_send![&*info, objectForKey: UIKeyboardFrameEndUserInfoKey];
            let Some(value) = value else { return };
            let frame: CGRect = msg_send![&*value, CGRectValue];
            let frame: CGRect = msg_send![self, convertRect: frame, fromView: None::<&AnyObject>];
            let bounds: CGRect = msg_send![self, bounds];
            ((bounds.origin.y + bounds.size.height - frame.origin.y).clamp(0.0, bounds.size.height), bounds.size.height)
        };
        self.ivars().keyboard.set(overlap as f32);
        let insets = self.safe_area();
        let visible = (overlap > 0.0).then_some((0.0, (height - overlap) as f32));
        self.with_surface(|surface| {
            surface.set_safe_area(insets);
            surface.set_keyboard(visible);
        });
        self.pace(Event::Wake);
    }

    /// The text size setting changed.
    pub(super) fn text_size_changed(&self) {
        self.with_surface(|surface| surface.set_text_scale(text_scale()));
        self.pace(Event::Wake);
    }

    pub(super) fn display_options_changed(&self) {
        self.with_surface(|surface| {
            surface.set_bold_text(bold_text());
            surface.set_increased_contrast(increased_contrast_with(contrast().is_some()));
        });
        self.pace(Event::Wake);
    }

    /// The Reduce Motion setting changed.
    pub(super) fn reduce_motion_changed(&self) {
        self.with_surface(|surface| surface.set_reduced_motion(reduced_motion()));
        self.pace(Event::Wake);
    }

    /// The system is short of memory: the surface frees what it can make again, and the app's
    /// `App::on_rest` runs. The frame on screen draws from nothing Rune freed, so that alone
    /// asks for no frame. A hook that changed what the view shows, or asked for work a frame
    /// does, wakes the pacing, which draws only for a view in a window in front.
    pub(super) fn memory_warned(&self) {
        // SAFETY: `UIView.window` is a nullable object.
        let window: Option<Retained<AnyObject>> = unsafe { msg_send![self, window] };
        let stale = self.with_surface(|surface| surface.memory_warning(window.is_some())).unwrap_or(false);
        if stale { self.pace(Event::Wake); }
    }

    /// The view's scene is coming back to the front.
    pub(super) fn entered_foreground(&self) {
        self.lifecycle(lifecycle::Event::Foreground);
        self.pace(Event::Foreground);
    }

    /// The view's scene went to the background.
    pub(super) fn entered_background(&self) {
        // The system sends no key-up to an app it puts away.
        self.keys(keys::Event::Reset);
        self.lifecycle(lifecycle::Event::Background);
        self.pace(Event::Background);
    }

    /// The view's scene takes input again.
    pub(super) fn became_active(&self) {
        self.lifecycle(lifecycle::Event::Active);
        // An active scene is in front, whether or not the notice of that came first.
        self.pace(Event::Foreground);
        self.pace(Event::Wake);
    }

    /// The view's scene is about to stop taking input: Control Center, the app switcher, a call.
    pub(super) fn resigning_active(&self) {
        self.lifecycle(lifecycle::Event::Inactive);
        // The frame that shows the focus gone, and gives the runtime the keys' release.
        self.pace(Event::Wake);
    }

    /// The app is about to terminate.
    pub(super) fn terminating(&self) {
        self.lifecycle(lifecycle::Event::Terminate);
    }
}
