//! An app that owns the screen: `UIApplicationMain`, one window scene, and a `RuneView` that
//! fills its window. The scene's configuration is made in code, so an app's `Info.plist` needs
//! a `UIApplicationSceneManifest` but no scene configuration in it and no storyboard.
//!
//! The links the scene is handed go to `App::on_open_url` the way a desktop window's do
//! (`open_url::incoming`): the ones the app was launched with (`scene:willConnectToSession:options:`)
//! and the ones that reach it while it runs (`scene:openURLContexts:` for a URL scheme,
//! `scene:continueUserActivity:` for a universal link).

#![allow(unsafe_code)]

use std::cell::RefCell;
use std::ffi::{c_char, c_int};

use objc2_foundation::NSString;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, class, define_class, extern_protocol, msg_send};

use super::links::{self, Activity};
use super::surface::{self, Mount};
use super::view::{RuneView, UIResponder};
use crate::app::App;

// SAFETY: the function has this C signature in UIKit's headers.
#[link(name = "UIKit", kind = "framework")]
unsafe extern "C" {
    fn UIApplicationMain(argc: c_int, argv: *mut *mut c_char, principal: Option<&NSString>, delegate: Option<&NSString>) -> c_int;
}

// SAFETY: the functions have these C signatures in `crt_externs.h`.
unsafe extern "C" {
    fn _NSGetArgc() -> *mut c_int;
    fn _NSGetArgv() -> *mut *mut *mut c_char;
}

thread_local! {
    /// The app `run` was given, until its scene connects.
    static MOUNT: RefCell<Option<Mount>> = const { RefCell::new(None) };
}

extern_protocol!(
    /// The methods below are declared where each delegate implements them.
    #[name = "UIApplicationDelegate"]
    unsafe trait ApplicationDelegate: NSObjectProtocol {}
);

extern_protocol!(
    /// UIKit refuses a scene delegate class that doesn't conform to `UISceneDelegate`, which
    /// this protocol inherits.
    #[name = "UIWindowSceneDelegate"]
    unsafe trait WindowSceneDelegate: NSObjectProtocol {}
);

define_class!(
    // SAFETY: `UIResponder` is made to be subclassed, and `AppDelegate` has no `Drop`.
    #[unsafe(super(UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RuneAppDelegate"]
    struct AppDelegate;

    // SAFETY: the superclass chain ends in `NSObject`, which implements the protocol.
    unsafe impl NSObjectProtocol for AppDelegate {}

    // SAFETY: every method of the protocol is optional.
    unsafe impl ApplicationDelegate for AppDelegate {}

    impl AppDelegate {
        /// The main menu (the iPad's menu bar): the system's menus with the app's items.
        #[unsafe(method(buildMenuWithBuilder:))]
        fn build_menu(&self, builder: &AnyObject) {
            crate::native_menu::build_ios_menus(builder);
        }

        #[unsafe(method_id(application:configurationForConnectingSceneSession:options:))]
        fn configuration(&self, _application: &AnyObject, session: &AnyObject, _options: &AnyObject) -> Retained<AnyObject> {
            // SAFETY: a session's role is an `NSString`, and `UISceneConfiguration`'s
            // initializer and `delegateClass` take these types.
            unsafe {
                let role: Retained<AnyObject> = msg_send![session, role];
                let configuration: Allocated<AnyObject> = msg_send![class!(UISceneConfiguration), alloc];
                let configuration: Retained<AnyObject> = msg_send![configuration, initWithName: &*NSString::from_str("Rune"), sessionRole: &*role];
                let _: () = msg_send![&*configuration, setDelegateClass: SceneDelegate::class()];
                configuration
            }
        }
    }
);

/// The scene's window, kept for as long as the scene has it.
struct SceneIvars {
    window: RefCell<Option<Retained<AnyObject>>>,
}

define_class!(
    // SAFETY: `UIResponder` is made to be subclassed, and `SceneDelegate` has no `Drop`.
    #[unsafe(super(UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RuneSceneDelegate"]
    #[ivars = SceneIvars]
    struct SceneDelegate;

    // SAFETY: the superclass chain ends in `NSObject`, which implements the protocol.
    unsafe impl NSObjectProtocol for SceneDelegate {}

    // SAFETY: every method of the protocol is optional.
    unsafe impl WindowSceneDelegate for SceneDelegate {}

    impl SceneDelegate {
        #[unsafe(method_id(init))]
        fn init(this: Allocated<Self>) -> Option<Retained<Self>> {
            let this = this.set_ivars(SceneIvars { window: RefCell::new(None) });
            // SAFETY: `init` is `UIResponder`'s designated initializer.
            unsafe { msg_send![super(this), init] }
        }

        #[unsafe(method(scene:willConnectToSession:options:))]
        fn will_connect(&self, scene: &AnyObject, _session: &AnyObject, options: &AnyObject) {
            // The links a launch carries arrive here, in the connection's options.
            // SAFETY: `options` is a `UISceneConnectionOptions`, whose `URLContexts` is an
            // `NSSet<UIOpenURLContext *>` and whose `userActivities` an `NSSet<NSUserActivity *>`.
            let (urls, activities) = unsafe {
                let contexts: Option<Retained<AnyObject>> = msg_send![options, URLContexts];
                let activities: Option<Retained<AnyObject>> = msg_send![options, userActivities];
                let activities = activities.map(|activities| members(&activities)).unwrap_or_default();
                (contexts.map(|contexts| opened(&contexts)).unwrap_or_default(), activities.iter().map(|activity| read(activity)).collect())
            };
            receive(urls, activities);
            // One app, one window: a second scene has nothing to show.
            let Some(mount) = MOUNT.take() else { return };
            let view = RuneView::new(MainThreadMarker::from(self), mount);
            // SAFETY: the scene of a window-scene session is a `UIWindowScene`; a view
            // controller's view and a window's root controller are settable objects.
            let window = unsafe {
                let window: Allocated<AnyObject> = msg_send![class!(UIWindow), alloc];
                let window: Retained<AnyObject> = msg_send![window, initWithWindowScene: scene];
                let controller: Retained<AnyObject> = msg_send![class!(UIViewController), new];
                let _: () = msg_send![&*controller, setView: &*view];
                let _: () = msg_send![&*window, setRootViewController: &*controller];
                let _: () = msg_send![&*window, makeKeyAndVisible];
                // The app's one view hears a hardware keyboard from the start.
                let _: bool = msg_send![&*view, becomeFirstResponder];
                window
            };
            *self.ivars().window.borrow_mut() = Some(window);
        }

        /// Links with a scheme the bundle declares, opened while the app runs.
        #[unsafe(method(scene:openURLContexts:))]
        fn open_url_contexts(&self, _scene: &AnyObject, contexts: &AnyObject) {
            // SAFETY: `contexts` is an `NSSet<UIOpenURLContext *>`.
            receive(unsafe { opened(contexts) }, Vec::new());
        }

        /// A universal link opened while the app runs, among the activities an app continues.
        #[unsafe(method(scene:continueUserActivity:))]
        fn continue_activity(&self, _scene: &AnyObject, activity: &AnyObject) {
            // SAFETY: `activity` is an `NSUserActivity`.
            receive(Vec::new(), vec![unsafe { read(activity) }]);
        }

        #[unsafe(method(sceneDidDisconnect:))]
        fn did_disconnect(&self, _scene: &AnyObject) {
            self.ivars().window.borrow_mut().take();
        }
    }
);

/// The members of an `NSSet`.
///
/// # Safety
/// `set` is an `NSSet`.
unsafe fn members(set: &AnyObject) -> Vec<Retained<AnyObject>> {
    // SAFETY: `allObjects` is an `NSArray`, whose indexes below `count` hold objects.
    unsafe {
        let all: Retained<AnyObject> = msg_send![set, allObjects];
        let count: usize = msg_send![&*all, count];
        (0..count).map(|index| msg_send![&*all, objectAtIndex: index]).collect()
    }
}

/// A URL as a string, or none for nil.
///
/// # Safety
/// `url` is an `NSURL` or nil.
unsafe fn string(url: Option<Retained<AnyObject>>) -> Option<String> {
    // SAFETY: an `NSURL`'s `absoluteString` is an `NSString` or nil.
    let string: Option<Retained<NSString>> = unsafe { msg_send![&*url?, absoluteString] };
    string.map(|string| string.to_string())
}

/// The URLs a set of `UIOpenURLContext`s opens.
///
/// # Safety
/// `contexts` is an `NSSet<UIOpenURLContext *>`.
unsafe fn opened(contexts: &AnyObject) -> Vec<String> {
    // SAFETY: a context's `URL` is an `NSURL`.
    unsafe { members(contexts).iter().filter_map(|context| string(msg_send![&**context, URL])).collect() }
}

/// An activity's type and web page.
///
/// # Safety
/// `activity` is an `NSUserActivity`.
unsafe fn read(activity: &AnyObject) -> Activity {
    // SAFETY: `activityType` is an `NSString`, and `webpageURL` an `NSURL` or nil.
    unsafe {
        let kind: Option<Retained<NSString>> = msg_send![activity, activityType];
        Activity { kind: kind.map(|kind| kind.to_string()).unwrap_or_default(), page: string(msg_send![activity, webpageURL]) }
    }
}

/// Hands the links in what a scene was given to the app's `on_open_url`, which wakes its view.
fn receive(urls: Vec<String>, activities: Vec<Activity>) {
    for link in links::incoming(urls, activities) { crate::open_url::incoming::receive(link); }
}

/// Runs `app` as the whole iOS app. Returns only when it can't start: `UIApplicationMain`
/// never does.
pub(crate) fn run<S: 'static>(app: App<S>) -> Result<(), String> {
    if MainThreadMarker::new().is_none() {
        return Err("an iOS app runs from the main thread".into());
    }
    MOUNT.set(Some(surface::mount(app)));
    // The app's one view listens by now if it has the hook; without it a link is dropped.
    crate::open_url::incoming::settle();
    // UIKit finds the delegates by name, so their classes are registered first.
    let _ = (AppDelegate::class(), SceneDelegate::class());
    // SAFETY: the process's own argument vector, and the name of a class registered above.
    unsafe { UIApplicationMain(*_NSGetArgc(), *_NSGetArgv(), None, Some(&NSString::from_str("RuneAppDelegate"))) };
    Err("UIApplicationMain returned".into())
}
