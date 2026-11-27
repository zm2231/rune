#![allow(unsafe_code)]

use std::cell::RefCell;
use std::ffi::{c_char, c_int, c_void};
use std::time::Duration;

use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{ClassType, MainThreadOnly, class, define_class, extern_protocol, msg_send};
use objc2_foundation::NSString;
use rune::time::SystemTime;

// SAFETY: the function has this C signature in UIKit's headers.
#[link(name = "UIKit", kind = "framework")]
unsafe extern "C" {
    fn UIApplicationMain(argc: c_int, argv: *mut *mut c_char, principal: Option<&NSString>, delegate: Option<&NSString>) -> c_int;
}

// SAFETY: the function has this C signature in QuartzCore's headers.
#[link(name = "QuartzCore", kind = "framework")]
unsafe extern "C" {
    fn CACurrentMediaTime() -> f64;
}

#[repr(C)]
struct Queue {
    _opaque: [u8; 0],
}

// SAFETY: the functions have these C signatures in `crt_externs.h` and libdispatch's headers,
// and `_dispatch_main_q` is the main queue's object, which lives for the process.
unsafe extern "C" {
    fn _NSGetArgc() -> *mut c_int;
    fn _NSGetArgv() -> *mut *mut *mut c_char;
    static _dispatch_main_q: Queue;
    fn dispatch_time(when: u64, delta: i64) -> u64;
    fn dispatch_after_f(when: u64, queue: *const Queue, context: *mut c_void, work: extern "C" fn(*mut c_void));
}

extern_protocol!(
    #[name = "UIApplicationDelegate"]
    unsafe trait ApplicationDelegate: NSObjectProtocol {}
);

extern_protocol!(
    #[name = "UIWindowSceneDelegate"]
    unsafe trait WindowSceneDelegate: NSObjectProtocol {}
);

type OnScene = Box<dyn FnOnce(&AnyObject)>;

thread_local! {
    static ON_SCENE: RefCell<Option<OnScene>> = const { RefCell::new(None) };
    static WINDOW: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
}

define_class!(
    // SAFETY: `NSObject` is made to be subclassed, and `AppDelegate` has no `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "IosPerfAppDelegate"]
    struct AppDelegate;

    // SAFETY: the superclass is `NSObject`, which implements the protocol.
    unsafe impl NSObjectProtocol for AppDelegate {}

    // SAFETY: every method of the protocol is optional.
    unsafe impl ApplicationDelegate for AppDelegate {}

    impl AppDelegate {
        #[unsafe(method_id(application:configurationForConnectingSceneSession:options:))]
        fn configuration(&self, _application: &AnyObject, session: &AnyObject, _options: &AnyObject) -> Retained<AnyObject> {
            // SAFETY: a session's role is an `NSString`, and `UISceneConfiguration`'s initializer
            // and `delegateClass` take these types.
            unsafe {
                let role: Retained<AnyObject> = msg_send![session, role];
                let configuration: Allocated<AnyObject> = msg_send![class!(UISceneConfiguration), alloc];
                let configuration: Retained<AnyObject> = msg_send![configuration, initWithName: &*NSString::from_str("IosPerf"), sessionRole: &*role];
                let _: () = msg_send![&*configuration, setDelegateClass: SceneDelegate::class()];
                configuration
            }
        }
    }
);

define_class!(
    // SAFETY: `NSObject` is made to be subclassed, and `SceneDelegate` has no `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "IosPerfSceneDelegate"]
    struct SceneDelegate;

    // SAFETY: the superclass is `NSObject`, which implements the protocol.
    unsafe impl NSObjectProtocol for SceneDelegate {}

    // SAFETY: every method of the protocol is optional.
    unsafe impl WindowSceneDelegate for SceneDelegate {}

    impl SceneDelegate {
        #[unsafe(method(scene:willConnectToSession:options:))]
        fn will_connect(&self, scene: &AnyObject, _session: &AnyObject, _options: &AnyObject) {
            if WINDOW.with(|w| w.borrow().is_some()) { return; }
            // SAFETY: `UIWindow`'s scene initializer and `UIViewController`'s properties take
            // and return these types, on the main thread.
            let (window, root) = unsafe {
                let window: Allocated<AnyObject> = msg_send![class!(UIWindow), alloc];
                let window: Retained<AnyObject> = msg_send![window, initWithWindowScene: scene];
                let controller: Retained<AnyObject> = msg_send![class!(UIViewController), new];
                let _: () = msg_send![&*window, setRootViewController: &*controller];
                let _: () = msg_send![&*window, makeKeyAndVisible];
                let root: Retained<AnyObject> = msg_send![&*controller, view];
                (window, root)
            };
            WINDOW.with(|w| *w.borrow_mut() = Some(window));
            if let Some(on_scene) = ON_SCENE.with(|s| s.borrow_mut().take()) { on_scene(&root); }
        }
    }
);

/// Runs a UIKit app with no Rune in it: its own `UIApplicationMain`, an app delegate that
/// configures each window scene in code (as a Rune app does), and a window whose root view
/// `on_scene` fills once the first scene connects.
pub fn run(on_scene: impl FnOnce(&AnyObject) + 'static) -> ! {
    ON_SCENE.with(|s| *s.borrow_mut() = Some(Box::new(on_scene)));
    let _ = (AppDelegate::class(), SceneDelegate::class());
    // SAFETY: called once, on the main thread, with the process's own arguments.
    unsafe { UIApplicationMain(*_NSGetArgc(), *_NSGetArgv(), None, Some(&NSString::from_str("IosPerfAppDelegate"))) };
    std::process::exit(1)
}

/// Runs `work` on the main queue `seconds` from now.
pub fn after(seconds: f64, work: impl FnOnce() + 'static) {
    extern "C" fn run(context: *mut c_void) {
        // SAFETY: `context` is the box `after` leaked, and the main queue runs this once.
        let work = unsafe { Box::from_raw(context.cast::<Box<dyn FnOnce()>>()) };
        work();
    }
    let work: Box<Box<dyn FnOnce()>> = Box::new(Box::new(work));
    let nanos = (seconds.max(0.0) * 1e9) as i64;
    // SAFETY: the main queue outlives the call, and `run` takes back the box exactly once.
    unsafe { dispatch_after_f(dispatch_time(0, nanos), &raw const _dispatch_main_q, Box::into_raw(work).cast(), run) };
}

/// The wall time of `media`, a time on `CACurrentMediaTime`'s clock (a display link's).
pub fn wall_of_media(media: f64) -> SystemTime {
    // SAFETY: `CACurrentMediaTime` reads the host's clock and takes nothing.
    let now = unsafe { CACurrentMediaTime() };
    let wall = rune::time::system_now();
    let gap = Duration::from_secs_f64((media - now).abs());
    if media >= now { wall + gap } else { wall - gap }
}
