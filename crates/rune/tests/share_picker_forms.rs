//! Which presentations of `NSSharingServicePicker` track as a menu rather than show a popover,
//! and how each menu form answers a dismissal. Opens plain pickers (not Rune's share) over a
//! Rune window in several setups and prints one line per setup. Opens windows, so it only runs
//! with RUNE_GUI_TESTS=1 in a logged-in GUI session.

#![allow(unsafe_code)]

#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
fn main() { mac::main() }

#[cfg(target_os = "macos")]
mod mac {
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Mutex, mpsc};
    use std::time::Duration;

    use block2::RcBlock;
    use objc2::rc::{Allocated, Retained};
    use objc2::runtime::{AnyObject, NSObject};
    use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, class, define_class, msg_send};
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};
    use objc2_foundation::{NSString, NSURL};
    use rune::*;

    struct Main;

    static CHOSE: Mutex<Vec<String>> = Mutex::new(Vec::new());
    static BEGUN: AtomicUsize = AtomicUsize::new(0);
    static ENDED: AtomicUsize = AtomicUsize::new(0);
    static MENUS: Mutex<Vec<String>> = Mutex::new(Vec::new());

    thread_local! {
        static TRACKED: RefCell<Vec<Retained<AnyObject>>> = const { RefCell::new(Vec::new()) };
        static PICKER: RefCell<Option<(Retained<AnyObject>, Retained<AnyObject>)>> = const { RefCell::new(None) };
        static CONTEXT: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
        static BASELINE: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
        static KEEP: RefCell<Vec<Retained<AnyObject>>> = const { RefCell::new(Vec::new()) };
    }

    fn record_choice(service: Option<&AnyObject>) {
        let title = service.map(|service| {
            // SAFETY: the picker passes an `NSSharingService`; `title` returns a non-null string.
            let title: Retained<NSString> = unsafe { msg_send![service, title] };
            title.to_string()
        });
        CHOSE.lock().unwrap().push(format!("{title:?}"));
    }

    define_class!(
        // SAFETY: `NSObject` has no subclassing requirements, and the class has no `Drop`.
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "RuneFormsPlainDelegate"]
        struct Plain;

        impl Plain {
            #[unsafe(method(sharingServicePicker:didChooseSharingService:))]
            fn chose(&self, _picker: &AnyObject, service: Option<&AnyObject>) { record_choice(service); }
        }
    );

    define_class!(
        // SAFETY: `NSObject` has no subclassing requirements, and the class has no `Drop`.
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "RuneFormsServicesDelegate"]
        #[ivars = bool]
        struct Services;

        impl Services {
            #[unsafe(method(sharingServicePicker:didChooseSharingService:))]
            fn chose(&self, _picker: &AnyObject, service: Option<&AnyObject>) { record_choice(service); }

            #[unsafe(method(sharingServicePicker:sharingServicesForItems:proposedSharingServices:))]
            fn services(&self, _picker: &AnyObject, _items: &AnyObject, proposed: &AnyObject) -> *mut AnyObject {
                if !*self.ivars() { return proposed as *const AnyObject as *mut AnyObject; }
                // SAFETY: `proposed` is an `NSArray`; the subarray is autoreleased and returned.
                unsafe {
                    let count: usize = msg_send![proposed, count];
                    let first: Retained<AnyObject> = msg_send![class!(NSMutableArray), array];
                    if count > 0 {
                        let item: *mut AnyObject = msg_send![proposed, objectAtIndex: 0usize];
                        let _: () = msg_send![&*first, addObject: item];
                    }
                    Retained::autorelease_return(first)
                }
            }
        }
    );

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFRunLoopCommonModes: *const c_void;
        fn CFRunLoopGetMain() -> *mut c_void;
        fn CFRunLoopPerformBlock(rl: *mut c_void, mode: *const c_void, block: &block2::DynBlock<dyn Fn()>);
        fn CFRunLoopWakeUp(rl: *mut c_void);
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGPreflightPostEventAccess() -> bool;
        fn CGEventCreateKeyboardEvent(source: *const c_void, key: u16, down: bool) -> *mut c_void;
        fn CGEventPostToPid(pid: i32, event: *mut c_void);
        fn CFRelease(object: *mut c_void);
    }

    fn on_main_async(f: impl FnOnce() + Send + 'static) {
        let f = Mutex::new(Some(f));
        let block = RcBlock::new(move || { if let Some(f) = f.lock().unwrap().take() { f() } });
        // SAFETY: the main run loop accepts blocks from any thread; common modes include menu
        // tracking, and the wake makes it run the block now.
        unsafe {
            let main = CFRunLoopGetMain();
            CFRunLoopPerformBlock(main, kCFRunLoopCommonModes, &block);
            CFRunLoopWakeUp(main);
        }
    }

    fn on_main<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = mpsc::channel();
        on_main_async(move || { tx.send(f()).ok(); });
        rx.recv_timeout(Duration::from_secs(5)).expect("the main run loop ran the block")
    }

    fn wait(limit: Duration, f: impl Fn() -> bool) -> bool {
        let start = rune::time::now();
        while !f() {
            if start.elapsed() > limit { return false; }
            std::thread::sleep(Duration::from_millis(10));
        }
        true
    }

    unsafe fn windows() -> Vec<usize> {
        // SAFETY: caller is on the main thread; reads `NSApp.windows`.
        unsafe {
            let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
            let windows: *mut AnyObject = msg_send![app, windows];
            let count: usize = msg_send![windows, count];
            (0..count).map(|i| { let w: *mut AnyObject = msg_send![windows, objectAtIndex: i]; w as usize }).collect()
        }
    }

    fn class_name(object: *mut AnyObject) -> String {
        // SAFETY: `object` is a live Objective-C object; `className` returns an `NSString`.
        unsafe {
            let class: *const NSString = msg_send![object, className];
            (*class).to_string()
        }
    }

    fn new_windows() -> Vec<String> {
        on_main(|| {
            let baseline = BASELINE.with(|b| b.borrow().clone());
            // SAFETY: main thread; each address is a live window of `NSApp.windows`.
            unsafe {
                windows().into_iter().filter(|w| !baseline.contains(w)).filter(|&w| {
                    let visible: bool = msg_send![w as *mut AnyObject, isVisible];
                    visible
                }).map(|w| class_name(w as *mut AnyObject)).collect()
            }
        })
    }

    fn observe_menus() {
        // SAFETY: main thread; the observer tokens are kept for the process.
        unsafe {
            let center: *mut AnyObject = msg_send![class!(NSNotificationCenter), defaultCenter];
            for (name, begin) in [("NSMenuDidBeginTrackingNotification", true), ("NSMenuDidEndTrackingNotification", false)] {
                let block = RcBlock::new(move |note: *mut AnyObject| {
                    let menu: *mut AnyObject = msg_send![note, object];
                    if begin {
                        BEGUN.fetch_add(1, Ordering::SeqCst);
                        let title: *const NSString = msg_send![menu, title];
                        let items: usize = msg_send![menu, numberOfItems];
                        let title = if title.is_null() { String::new() } else { (*title).to_string() };
                        MENUS.lock().unwrap().push(format!("{} '{title}' {items} items", class_name(menu)));
                        TRACKED.with(|t| t.borrow_mut().extend(Retained::retain(menu)));
                    } else {
                        ENDED.fetch_add(1, Ordering::SeqCst);
                        TRACKED.with(|t| t.borrow_mut().retain(|m| !std::ptr::eq(&**m, menu)));
                    }
                });
                let token: Option<Retained<AnyObject>> = msg_send![center, addObserverForName: &*NSString::from_str(name),
                    object: std::ptr::null::<AnyObject>(), queue: std::ptr::null::<AnyObject>(), usingBlock: &*block];
                KEEP.with(|k| k.borrow_mut().extend(token));
            }
        }
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Setup { Plain, Proposed, OneService, DuringMenu, Inactive, ShareMenuItem }

    fn delegate(setup: Setup) -> Retained<AnyObject> {
        let mtm = MainThreadMarker::new().unwrap();
        // SAFETY: `init` is `NSObject`'s designated initializer for both delegate classes.
        unsafe {
            match setup {
                Setup::Proposed | Setup::OneService => {
                    let d: Retained<Services> = msg_send![super(mtm.alloc::<Services>().set_ivars(setup == Setup::OneService)), init];
                    Retained::into_super(Retained::into_super(d))
                }
                _ => {
                    let d: Retained<Plain> = msg_send![super(mtm.alloc::<Plain>().set_ivars(())), init];
                    Retained::into_super(Retained::into_super(d))
                }
            }
        }
    }

    fn make_picker(setup: Setup) -> Retained<AnyObject> {
        let delegate = delegate(setup);
        // SAFETY: a picker over one URL item, with a delegate that answers its methods; both are
        // kept in PICKER until the setup ends.
        unsafe {
            let items: Retained<AnyObject> = msg_send![class!(NSMutableArray), array];
            let url = NSURL::URLWithString(&NSString::from_str("https://example.com/")).unwrap();
            let _: () = msg_send![&*items, addObject: &*url];
            let picker: Allocated<AnyObject> = msg_send![class!(NSSharingServicePicker), alloc];
            let picker: Retained<AnyObject> = msg_send![picker, initWithItems: &*items];
            let _: () = msg_send![&*picker, setDelegate: &*delegate];
            PICKER.with(|p| *p.borrow_mut() = Some((picker.clone(), delegate)));
            picker
        }
    }

    fn content_view() -> *mut AnyObject {
        let baseline = BASELINE.with(|b| b.borrow().clone());
        let window = baseline.into_iter().find(|&w| class_name(w as *mut AnyObject).contains("WinitWindow")).expect("the Rune window");
        // SAFETY: a live window; `contentView` returns its view.
        unsafe { msg_send![window as *mut AnyObject, contentView] }
    }

    fn show_relative(picker: &AnyObject) {
        let view = content_view();
        // SAFETY: shows the picker below a small rect near the view's top left.
        unsafe {
            let rect = CGRect::new(CGPoint::new(40.0, 40.0), CGSize::new(120.0, 24.0));
            let _: () = msg_send![picker, showRelativeToRect: rect, ofView: view, preferredEdge: 1usize];
        }
    }

    fn pop_up(menu: Retained<AnyObject>) {
        let view = content_view();
        // SAFETY: pops `menu` up in the Rune window's content view; this tracks until dismissed.
        unsafe {
            let _: bool = msg_send![&*menu, popUpMenuPositioningItem: std::ptr::null::<AnyObject>(), atLocation: CGPoint::new(60.0, 60.0), inView: view];
        }
    }

    fn open(setup: Setup) {
        on_main_async(move || {
            let picker = make_picker(setup);
            match setup {
                Setup::Plain | Setup::Proposed | Setup::OneService => show_relative(&picker),
                Setup::Inactive => {
                    // SAFETY: main thread; `deactivate` gives up active status.
                    unsafe {
                        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
                        let _: () = msg_send![app, deactivate];
                    }
                    show_relative(&picker);
                }
                Setup::DuringMenu => {
                    // SAFETY: a context menu with one item, popped up; the picker opens from a
                    // block while it tracks.
                    let menu: Retained<AnyObject> = unsafe {
                        let menu: Allocated<AnyObject> = msg_send![class!(NSMenu), alloc];
                        let menu: Retained<AnyObject> = msg_send![menu, initWithTitle: &*NSString::from_str("context")];
                        let _: *mut AnyObject = msg_send![&*menu, addItemWithTitle: &*NSString::from_str("Item"), action: None::<objc2::runtime::Sel>, keyEquivalent: &*NSString::from_str("")];
                        menu
                    };
                    CONTEXT.with(|c| *c.borrow_mut() = Some(menu.clone()));
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(400));
                        on_main_async(move || {
                            let picker = PICKER.with(|p| p.borrow().as_ref().map(|(p, _)| p.clone())).unwrap();
                            show_relative(&picker);
                        });
                    });
                    pop_up(menu);
                }
                Setup::ShareMenuItem => {
                    // SAFETY: `standardShareMenuItem` (macOS 13) returns an item that shows this
                    // picker's services; it goes into a new menu, popped up.
                    let menu: Option<Retained<AnyObject>> = unsafe {
                        let responds: bool = msg_send![&*picker, respondsToSelector: objc2::sel!(standardShareMenuItem)];
                        if responds {
                            let item: *mut AnyObject = msg_send![&*picker, standardShareMenuItem];
                            let menu: Allocated<AnyObject> = msg_send![class!(NSMenu), alloc];
                            let menu: Retained<AnyObject> = msg_send![menu, initWithTitle: &*NSString::from_str("share item")];
                            let _: () = msg_send![&*menu, addItem: item];
                            Some(menu)
                        } else { None }
                    };
                    match menu {
                        Some(menu) => pop_up(menu),
                        None => println!("share_picker_forms: standardShareMenuItem isn't available on this macOS"),
                    }
                }
            }
        });
    }

    fn escape_key() {
        for down in [true, false] {
            // SAFETY: a null source makes an event from the default state; 53 is Escape. The
            // event goes to this process only, then is released once.
            unsafe {
                let event = CGEventCreateKeyboardEvent(std::ptr::null(), 53, down);
                assert!(!event.is_null());
                CGEventPostToPid(std::process::id() as i32, event);
                CFRelease(event);
            }
        }
    }

    fn tidy() {
        on_main(|| {
            for menu in TRACKED.with(|t| t.borrow().clone()) {
                // SAFETY: a menu that is tracking; this ends its tracking.
                unsafe { let _: () = msg_send![&*menu, cancelTrackingWithoutAnimation]; }
            }
            if let Some((picker, _)) = PICKER.with(|p| p.borrow().clone()) {
                // SAFETY: `close` (macOS 13) dismisses a popover picker; checked first.
                unsafe {
                    let closes: bool = msg_send![&*picker, respondsToSelector: objc2::sel!(close)];
                    if closes { let _: () = msg_send![&*picker, close]; }
                }
            }
        });
        std::thread::sleep(Duration::from_millis(500));
        on_main(|| {
            if let Some(menu) = CONTEXT.with(|c| c.borrow_mut().take()) {
                // SAFETY: as above.
                unsafe { let _: () = msg_send![&*menu, cancelTrackingWithoutAnimation]; }
            }
            PICKER.with(|p| p.borrow_mut().take());
            // SAFETY: main thread; brings this app back to the front for the next setup.
            unsafe {
                let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
                let _: () = msg_send![app, activateIgnoringOtherApps: true];
            }
        });
        std::thread::sleep(Duration::from_millis(500));
    }

    fn probe(setup: Setup, name: &str, can_post: bool) {
        CHOSE.lock().unwrap().clear();
        MENUS.lock().unwrap().clear();
        let (begun, ended) = (BEGUN.load(Ordering::SeqCst), ENDED.load(Ordering::SeqCst));
        let expected_menus = if setup == Setup::DuringMenu { 2 } else { 1 };
        open(setup);
        let shown = wait(Duration::from_secs(3), || BEGUN.load(Ordering::SeqCst) >= begun + expected_menus || !new_windows().is_empty());
        std::thread::sleep(Duration::from_millis(300));
        let windows = new_windows();
        let menus = MENUS.lock().unwrap().clone();
        let tracking = BEGUN.load(Ordering::SeqCst) - begun > ENDED.load(Ordering::SeqCst) - ended;
        let form = if !shown { "nothing".to_owned() } else if !windows.is_empty() { format!("window {windows:?}") } else { "menu".to_owned() };
        let escape = if !can_post {
            "skipped (no Accessibility permission)".to_owned()
        } else if form == "nothing" {
            "skipped".to_owned()
        } else {
            let (count, ended_before) = (CHOSE.lock().unwrap().len(), ENDED.load(Ordering::SeqCst));
            escape_key();
            std::thread::sleep(Duration::from_millis(800));
            format!("{} menu(s) ended, {} answer(s), {} new window(s) left", ENDED.load(Ordering::SeqCst) - ended_before, CHOSE.lock().unwrap().len() - count, new_windows().len())
        };
        tidy();
        let chose = CHOSE.lock().unwrap().clone();
        println!("share_picker_forms: {name}: form {form}; menus begun {menus:?}; tracking after show {tracking}; Escape: {escape}; answers {chose:?}");
    }

    pub fn main() {
        if std::env::var_os("RUNE_GUI_TESTS").is_none() {
            println!("share_picker_forms: skipped (set RUNE_GUI_TESTS=1 in a GUI session)");
            return;
        }
        std::thread::spawn(|| { std::thread::sleep(Duration::from_secs(120)); eprintln!("timeout"); std::process::exit(2) });
        rune::orderly_exit();
        let spec = AppSpec::new("share forms", (320.0, 200.0), || Main, |_: &Main, _: &mut Cx<Main>| el());
        let (ready_tx, ready_rx) = mpsc::channel();
        let app = App::new(spec).args(Vec::new()).on_ready(move |_| { ready_tx.send(()).ok(); });
        let handle = app.handle();
        let worker = std::thread::spawn(move || {
            ready_rx.recv_timeout(Duration::from_secs(60)).expect("window ready");
            std::thread::sleep(Duration::from_millis(500));
            on_main(|| {
                observe_menus();
                // SAFETY: main thread.
                let now = unsafe { windows() };
                BASELINE.with(|b| *b.borrow_mut() = now);
            });
            // SAFETY: takes no arguments.
            let can_post = unsafe { CGPreflightPostEventAccess() };
            let os = on_main(|| {
                // SAFETY: `processInfo` and its version string.
                unsafe {
                    let info: *mut AnyObject = msg_send![class!(NSProcessInfo), processInfo];
                    let v: *const NSString = msg_send![info, operatingSystemVersionString];
                    (*v).to_string()
                }
            });
            println!("share_picker_forms: {os}");
            for (setup, name) in [
                (Setup::Plain, "plain delegate"),
                (Setup::Proposed, "delegate returns the proposed services"),
                (Setup::OneService, "delegate returns one service"),
                (Setup::DuringMenu, "shown while a context menu tracks"),
                (Setup::Inactive, "shown while the app is inactive"),
                (Setup::ShareMenuItem, "standardShareMenuItem in a popped-up menu"),
            ] {
                probe(setup, name, can_post);
            }
            handle.quit();
        });
        app.run();
        worker.join().ok();
        println!("share_picker_forms: done");
    }
}
