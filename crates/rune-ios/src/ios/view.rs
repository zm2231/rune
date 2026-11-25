use std::cell::{Cell, OnceCell, RefCell};
use std::ffi::c_void;

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_core_foundation::CGRect;
use objc2_foundation::{NSNotification, NSNotificationCenter, NSSet};
use objc2_quartz_core::CADisplayLink;
use objc2_ui_kit::{
    UIAccessibilityIsReduceMotionEnabled, UIAccessibilityReduceMotionStatusDidChangeNotification, UIEvent, UIResponder,
    UITouch, UITraitCollection, UITraitEnvironment, UIUserInterfaceStyle, UIView,
};
use rune::{AppSpec, TouchPhase};

use super::haptics::Haptics;
use super::host::{Factory, Host, Mount, factory};
use super::link::{Link, arm_timer, waker};
use super::touch;
use crate::pacing::{Pacer, Pacing, PacingEffect, PacingEvent, step};

/// Whether the view's runtime has started.
enum Mounted {
    /// No size yet: the runtime starts at the first layout with one.
    Waiting(Factory),
    Running(Box<dyn Host>),
    /// The layer couldn't take a Metal surface; the view stays blank.
    Failed,
}

#[doc(hidden)]
pub struct Ivars {
    mounted: RefCell<Mounted>,
    link: OnceCell<Link>,
    pacer: Cell<Pacer>,
    haptics: Haptics,
}

define_class!(
    /// A `UIView` whose layer is a `CAMetalLayer` that a Rune runtime draws into. [`crate::run`]
    /// fills each scene's window with one; an app with its own UIKit hierarchy can place one
    /// anywhere with [`RuneView::new`].
    #[unsafe(super(UIView, UIResponder))]
    #[thread_kind = MainThreadOnly]
    #[name = "RuneView"]
    #[ivars = Ivars]
    pub struct RuneView;

    impl RuneView {
        #[unsafe(method(layerClass))]
        fn layer_class() -> &'static AnyClass {
            AnyClass::get(c"CAMetalLayer").unwrap_or_else(|| unreachable!("QuartzCore defines CAMetalLayer"))
        }

        #[unsafe(method(didMoveToWindow))]
        fn did_move_to_window(&self) {
            // SAFETY: `UIView` implements `didMoveToWindow`, taking and returning nothing.
            let () = unsafe { msg_send![super(self), didMoveToWindow] };
            if self.window().is_some() {
                self.attach();
            }
        }

        #[unsafe(method(layoutSubviews))]
        fn layout_subviews(&self) {
            // SAFETY: `UIView` implements `layoutSubviews`, taking and returning nothing.
            let () = unsafe { msg_send![super(self), layoutSubviews] };
            self.laid_out();
        }

        #[unsafe(method(safeAreaInsetsDidChange))]
        fn safe_area_insets_did_change(&self) {
            // SAFETY: `UIView` implements `safeAreaInsetsDidChange`, taking and returning nothing.
            let () = unsafe { msg_send![super(self), safeAreaInsetsDidChange] };
            let insets = self.safe_area();
            self.with_host(|host| host.set_safe_area(insets));
        }

        #[unsafe(method(touchesBegan:withEvent:))]
        fn touches_began(&self, touches: &NSSet<UITouch>, event: Option<&UIEvent>) {
            self.ivars().haptics.prepare(self);
            self.touches(touches, event, TouchPhase::Started);
        }

        #[unsafe(method(touchesMoved:withEvent:))]
        fn touches_moved(&self, touches: &NSSet<UITouch>, event: Option<&UIEvent>) {
            self.touches(touches, event, TouchPhase::Moved);
        }

        #[unsafe(method(touchesEnded:withEvent:))]
        fn touches_ended(&self, touches: &NSSet<UITouch>, event: Option<&UIEvent>) {
            self.touches(touches, event, TouchPhase::Ended);
        }

        #[unsafe(method(touchesCancelled:withEvent:))]
        fn touches_cancelled(&self, touches: &NSSet<UITouch>, event: Option<&UIEvent>) {
            self.touches(touches, event, TouchPhase::Cancelled);
        }

        #[unsafe(method(runeColorTraitsChanged))]
        fn color_traits_changed(&self) {
            let dark = self.dark();
            self.with_host(|host| host.set_dark(dark));
        }

        #[unsafe(method(runeReduceMotionChanged:))]
        fn reduce_motion_changed(&self, _note: &NSNotification) {
            let reduced = UIAccessibilityIsReduceMotionEnabled();
            self.with_host(|host| host.set_reduced_motion(reduced));
        }
    }
);

impl RuneView {
    /// A view that runs `spec` once it is laid out with a size. It starts the app's state with
    /// `spec.init`.
    pub fn new<S: 'static>(mtm: MainThreadMarker, spec: AppSpec<S>) -> Retained<Self> {
        Self::with_factory(mtm, factory(spec))
    }

    pub(crate) fn with_factory(mtm: MainThreadMarker, factory: Factory) -> Retained<Self> {
        let ivars = Ivars {
            mounted: RefCell::new(Mounted::Waiting(factory)),
            link: OnceCell::new(),
            pacer: Cell::new(Pacer::default()),
            haptics: Haptics::default(),
        };
        let this = mtm.alloc::<Self>().set_ivars(ivars);
        // SAFETY: `UIView`'s designated initializer, on an allocated view whose ivars are set.
        let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: CGRect::default()] };
        this.setMultipleTouchEnabled(true);
        this
    }

    /// The app left the foreground: Metal may not draw there, so the display link stops.
    pub(crate) fn enter_background(&self) {
        self.pace(PacingEvent::Background);
    }

    pub(crate) fn enter_foreground(&self) {
        self.pace(PacingEvent::Foreground);
    }

    /// One display refresh: draws a frame, plays the haptics it asked for, and lets the pacer
    /// decide whether the link keeps ticking.
    pub(crate) fn tick(&self, link: &CADisplayLink) {
        let out = self.with_host(|host| (host.frame(), host.take_haptics()));
        let (animating, wake_after) = match out {
            Some((frame, haptics)) => {
                for haptic in haptics {
                    self.ivars().haptics.play(self, haptic);
                }
                (frame.animating, frame.wake_after_secs)
            }
            None => (false, None),
        };
        let now = link.timestamp();
        let period = (link.targetTimestamp() - now).max(0.0);
        self.pace(PacingEvent::Framed { animating, wake_after, now, period });
    }

    pub(crate) fn pace(&self, event: PacingEvent) {
        let mut pacer = self.ivars().pacer.get();
        let effects = step(&mut pacer, event);
        self.ivars().pacer.set(pacer);
        for effect in effects {
            match effect {
                PacingEffect::Resume => self.set_link_paused(false),
                PacingEffect::Pause => self.set_link_paused(true),
                PacingEffect::ArmTimer { attempt, after } => arm_timer(self, self.mtm(), attempt, after),
            }
        }
    }

    fn set_link_paused(&self, paused: bool) {
        if let Some(link) = self.ivars().link.get() {
            link.set_paused(paused);
        }
    }

    /// In a window: take the screen's scale, start the display link, and follow the system's
    /// appearance and Reduce Motion.
    fn attach(&self) {
        // SAFETY: reading a trait collection's display scale has no further requirements.
        let scale = unsafe { self.traitCollection().displayScale() };
        if scale > 0.0 {
            self.setContentScaleFactor(scale);
        }
        let mtm = self.mtm();
        let mut first = false;
        self.ivars().link.get_or_init(|| {
            first = true;
            Link::new(self, mtm, self.ivars().pacer.get().state != Pacing::Running)
        });
        if first {
            self.observe_system();
        }
        self.pace(PacingEvent::Wake);
    }

    fn observe_system(&self) {
        // SAFETY: `systemTraitsAffectingColorAppearance` (iOS 17) returns an array of trait
        // classes, and `registerForTraitChanges:withTarget:action:` takes it with a target that
        // answers the zero-argument action. UIKit holds the target weakly and drops the
        // registration with the view.
        unsafe {
            let traits: Retained<AnyObject> = msg_send![UITraitCollection::class(), systemTraitsAffectingColorAppearance];
            let _: Option<Retained<AnyObject>> = msg_send![
                self,
                registerForTraitChanges: &*traits,
                withTarget: self,
                action: sel!(runeColorTraitsChanged)
            ];
        }
        // SAFETY: the view answers the one-argument selector with an `NSNotification`, and the
        // notification centre stops calling an observer once it is deallocated.
        unsafe {
            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                self,
                sel!(runeReduceMotionChanged:),
                Some(UIAccessibilityReduceMotionStatusDidChangeNotification),
                None,
            );
        }
    }

    fn laid_out(&self) {
        let size = self.bounds().size;
        let viewport = (size.width as f32, size.height as f32);
        let scale = self.contentScaleFactor() as f32;
        let waiting = match &*self.ivars().mounted.borrow() {
            Mounted::Waiting(factory) => Some(factory.clone()),
            Mounted::Running(_) | Mounted::Failed => None,
        };
        let Some(factory) = waiting else {
            self.with_host(|host| host.resize(viewport, scale));
            self.pace(PacingEvent::Wake);
            return;
        };
        if viewport.0 <= 0.0 || viewport.1 <= 0.0 {
            return;
        }
        let mount = Mount {
            layer: Retained::as_ptr(&self.layer()) as *mut c_void,
            viewport,
            scale,
            dark: self.dark(),
            wake: waker(self, self.mtm()),
        };
        let next = match factory(mount) {
            Ok(mut host) => {
                host.set_safe_area(self.safe_area());
                host.set_reduced_motion(UIAccessibilityIsReduceMotionEnabled());
                Mounted::Running(host)
            }
            Err(error) => {
                eprintln!("rune-ios: the view's layer took no Metal surface: {error}");
                Mounted::Failed
            }
        };
        *self.ivars().mounted.borrow_mut() = next;
        self.pace(PacingEvent::Wake);
    }

    fn touches(&self, touches: &NSSet<UITouch>, event: Option<&UIEvent>, phase: TouchPhase) {
        self.with_host(|host| touch::feed(host, self, touches, event, phase));
        self.pace(PacingEvent::Wake);
    }

    fn with_host<R>(&self, f: impl FnOnce(&mut dyn Host) -> R) -> Option<R> {
        match &mut *self.ivars().mounted.borrow_mut() {
            Mounted::Running(host) => Some(f(host.as_mut())),
            Mounted::Waiting(_) | Mounted::Failed => None,
        }
    }

    /// Top, right, bottom, left, as `Runtime::set_safe_area` takes them.
    fn safe_area(&self) -> [f32; 4] {
        let i = self.safeAreaInsets();
        [i.top as f32, i.right as f32, i.bottom as f32, i.left as f32]
    }

    fn dark(&self) -> bool {
        // SAFETY: reading a trait collection's interface style has no further requirements.
        unsafe { self.traitCollection().userInterfaceStyle() == UIUserInterfaceStyle::Dark }
    }
}
