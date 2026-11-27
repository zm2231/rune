//! An app that owns the screen: `UIApplicationMain`, its window scenes, and a `RuneView` that
//! fills each window (`scenes`). The scene's configuration is made in code, so an app's
//! `Info.plist` needs a `UIApplicationSceneManifest` but no scene configuration in it and no
//! storyboard.
//!
//! The links the scene is handed go to `App::on_open_url` the way a desktop window's do
//! (`open_url::incoming`): the ones the app was launched with (`scene:willConnectToSession:options:`)
//! and the ones that reach it while it runs (`scene:openURLContexts:` for a URL scheme,
//! `scene:continueUserActivity:` for a universal link).

#![allow(unsafe_code)]

use std::ffi::{c_char, c_int};

use objc2_foundation::NSString;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{ClassType, MainThreadMarker, MainThreadOnly, class, define_class, extern_protocol, msg_send};

use super::links::{self, Activity};
use super::mount::Entry;
use super::scenes;
use super::view::UIResponder;
use crate::platform::objc_boundary::guarded;
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
            guarded("buildMenuWithBuilder:", || { crate::native_menu::build_ios_menus(builder); })
        }

        #[unsafe(method_id(application:configurationForConnectingSceneSession:options:))]
        fn configuration(&self, _application: &AnyObject, session: &AnyObject, _options: &AnyObject) -> Retained<AnyObject> {
            guarded("application:configurationForConnectingSceneSession:options:", || {
                // SAFETY: a session's role is an `NSString`, and `UISceneConfiguration`'s
                // initializer and `delegateClass` take these types.
                unsafe {
                    let role: Retained<AnyObject> = msg_send![session, role];
                    let configuration: Allocated<AnyObject> = msg_send![class!(UISceneConfiguration), alloc];
                    let configuration: Retained<AnyObject> = msg_send![configuration, initWithName: &*NSString::from_str("Rune"), sessionRole: &*role];
                    let _: () = msg_send![&*configuration, setDelegateClass: SceneDelegate::class()];
                    configuration
                }
            })
        }

        #[unsafe(method(application:didDiscardSceneSessions:))]
        fn discarded(&self, _application: &AnyObject, sessions: &AnyObject) {
            guarded("application:didDiscardSceneSessions:", || {
                // SAFETY: `sessions` is an `NSSet<UISceneSession *>`.
                for session in unsafe { members(sessions) } {
                    if let Some(key) = scenes::session_key(&session) { scenes::discarded(key); }
                }
            })
        }
    }
);

define_class!(
    // SAFETY: `UIResponder` is made to be subclassed, and `SceneDelegate` has no `Drop`.
    #[unsafe(super(UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RuneSceneDelegate"]
    struct SceneDelegate;

    // SAFETY: the superclass chain ends in `NSObject`, which implements the protocol.
    unsafe impl NSObjectProtocol for SceneDelegate {}

    // SAFETY: every method of the protocol is optional.
    unsafe impl WindowSceneDelegate for SceneDelegate {}

    impl SceneDelegate {
        #[unsafe(method(scene:willConnectToSession:options:))]
        fn will_connect(&self, scene: &AnyObject, session: &AnyObject, options: &AnyObject) {
            guarded("scene:willConnectToSession:options:", || {
                // The links a launch carries arrive here, in the connection's options.
                // SAFETY: `options` is a `UISceneConnectionOptions`, whose `URLContexts` is an
                // `NSSet<UIOpenURLContext *>` and whose `userActivities` an `NSSet<NSUserActivity *>`.
                let (urls, activities) = unsafe {
                    let contexts: Option<Retained<AnyObject>> = msg_send![options, URLContexts];
                    let activities: Option<Retained<AnyObject>> = msg_send![options, userActivities];
                    let activities = activities.map(|activities| members(&activities)).unwrap_or_default();
                    (contexts.map(|contexts| opened(&contexts)).unwrap_or_default(), activities)
                };
                let key = scenes::session_key(session).or_else(|| activities.iter().find_map(|activity| scenes::activity_key(activity)));
                // SAFETY: each member of `userActivities` is an `NSUserActivity`.
                receive(urls, activities.iter().map(|activity| unsafe { read(activity) }).collect());
                // SAFETY: as above.
                for activity in &activities { unsafe { crate::handoff::ios::continued(activity) }; }
                scenes::connected(scene, key);
            })
        }

        /// Links with a scheme the bundle declares, opened while the app runs.
        #[unsafe(method(scene:openURLContexts:))]
        fn open_url_contexts(&self, _scene: &AnyObject, contexts: &AnyObject) {
            guarded("scene:openURLContexts:", || {
                // SAFETY: `contexts` is an `NSSet<UIOpenURLContext *>`.
                receive(unsafe { opened(contexts) }, Vec::new());
            })
        }

        /// A universal link opened, or a Handoff activity continued, while the app runs.
        #[unsafe(method(scene:continueUserActivity:))]
        fn continue_activity(&self, _scene: &AnyObject, activity: &AnyObject) {
            guarded("scene:continueUserActivity:", || {
                // SAFETY: `activity` is an `NSUserActivity`.
                receive(Vec::new(), vec![unsafe { read(activity) }]);
                // SAFETY: as above.
                unsafe { crate::handoff::ios::continued(activity) };
            })
        }

        #[unsafe(method(sceneDidDisconnect:))]
        fn did_disconnect(&self, scene: &AnyObject) {
            guarded("sceneDidDisconnect:", || { scenes::disconnected(scene); })
        }

        #[unsafe(method(sceneWillEnterForeground:))]
        fn will_enter_foreground(&self, scene: &AnyObject) {
            guarded("sceneWillEnterForeground:", || { scenes::foreground(scene); })
        }

        #[unsafe(method(sceneDidEnterBackground:))]
        fn did_enter_background(&self, scene: &AnyObject) {
            guarded("sceneDidEnterBackground:", || { scenes::background(scene); })
        }
    }
);

/// The members of an `NSSet`.
///
/// # Safety
/// `set` is an `NSSet`.
pub(super) unsafe fn members(set: &AnyObject) -> Vec<Retained<AnyObject>> {
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

/// Runs `app` as the whole iOS app, with the windows it adds. Returns only when it can't start:
/// `UIApplicationMain` never does.
pub(crate) fn run<S: 'static>(app: App<S>) -> Result<(), String> {
    run_windows(Box::new(move |_| vec![crate::WindowSpec::from_app("main", app).into()]))
}

/// Runs the windows a `run_windows` factory returns as the whole iOS app: the first is the
/// primary, which fills the launch scene, and the others show as `scenes` says.
pub(crate) fn run_windows(factory: Box<dyn FnOnce(&[crate::Display]) -> Vec<crate::AnyWindowSpec>>) -> Result<(), String> {
    let Some(mtm) = MainThreadMarker::new() else { return Err("an iOS app runs from the main thread".into()) };
    let windows = crate::windows::flatten(factory(&[screen(mtm)]));
    crate::windows::validate(&windows)?;
    if windows.is_empty() { return Err("the app created no windows".into()); }
    run_entries(windows.into_iter().map(crate::AnyWindowSpec::entry).collect())
}

/// The main screen as a display: its whole size in pixels at its native scale.
fn screen(_: MainThreadMarker) -> crate::Display {
    // SAFETY: `UIScreen.mainScreen` is a screen, read on the main thread, whose `nativeBounds`
    // is a `CGRect` and `nativeScale` a `CGFloat`.
    let (bounds, scale) = unsafe {
        let screen: Retained<AnyObject> = msg_send![class!(UIScreen), mainScreen];
        let bounds: objc2_core_foundation::CGRect = msg_send![&*screen, nativeBounds];
        let scale: objc2_core_foundation::CGFloat = msg_send![&*screen, nativeScale];
        (bounds, scale)
    };
    crate::Display { id: None, name: None, position: (0, 0), size: (bounds.size.width as u32, bounds.size.height as u32), scale, primary: true }
}

fn run_entries(entries: Vec<Entry>) -> Result<(), String> {
    scenes::start(entries);
    // The app's views listen by now if they have the hook; without it a link is dropped.
    crate::open_url::incoming::settle();
    // UIKit finds the delegates by name, so their classes are registered first.
    let _ = (AppDelegate::class(), SceneDelegate::class());
    // SAFETY: the process's own argument vector, and the name of a class registered above.
    unsafe { UIApplicationMain(*_NSGetArgc(), *_NSGetArgv(), None, Some(&NSString::from_str("RuneAppDelegate"))) };
    Err("UIApplicationMain returned".into())
}
