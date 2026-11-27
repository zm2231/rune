//! The share picker opens without stopping Rune: while it is on screen, a spinner in a second
//! window keeps drawing. A click outside the picker (macOS shows it as a popover) dismisses it and
//! reaches the callback exactly once, and hiding the window it shows over answers its share. Where
//! the picker tracks as a menu instead, the test cancels the menu through AppKit and says so.
//! With permission to post system events (Accessibility), it then presses a real Escape, a
//! CoreGraphics key event posted to this process only, first with Rune's window key and then with
//! the picker's window made key through AppKit, and prints where the key went and whether the
//! picker answered; those runs only report. Opens windows, so it only runs
//! with RUNE_GUI_TESTS=1 in a logged-in GUI session.

#![allow(unsafe_code)]

#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
fn main() { mac::main() }

#[cfg(target_os = "macos")]
mod mac {
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Mutex, mpsc};
    use std::time::Duration;

    use std::ffi::c_void;

    use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
    use objc2_foundation::NSString;
    use rune::*;

    struct Main;
    struct Second;

    static MAIN_FRAMES: AtomicUsize = AtomicUsize::new(0);
    static SECOND_FRAMES: AtomicUsize = AtomicUsize::new(0);
    static TRACKING: AtomicBool = AtomicBool::new(false);
    static ANSWERS: Mutex<Vec<String>> = Mutex::new(Vec::new());

    thread_local! {
        static MENU: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
        static OBSERVERS: RefCell<Vec<Retained<AnyObject>>> = const { RefCell::new(Vec::new()) };
        static BASELINE: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    }

    fn spinner() -> El {
        el().key("spinner").size(24.0, 24.0).pin(Some(40.0), Some(40.0), None, None)
            .fill(oklch(0.7, 0.15, 50.0)).looping(LoopSpec::rotation(1.0))
    }

    fn main_view(_: &Main, _: &mut Cx<Main>) -> El {
        el()
            .child(el().key("share").size(120.0, 24.0).pin(Some(40.0), Some(100.0), None, None)
                .fill(oklch(0.45, 0.02, 260.0)).radius(6.0))
            .child(spinner())
    }

    fn second_view(_: &Second, _: &mut Cx<Second>) -> El { el().child(spinner()) }

    fn main_poll(_: &mut Main, _: f64, _: &mut Tx) -> bool {
        MAIN_FRAMES.fetch_add(1, Ordering::SeqCst);
        false
    }

    fn second_poll(_: &mut Second, _: f64, _: &mut Tx) -> bool {
        SECOND_FRAMES.fetch_add(1, Ordering::SeqCst);
        false
    }

    fn share(_: &mut Main, tx: &mut Tx) {
        tx.share(Share::new().url("https://example.com/"), "share", |_: &mut Main, result, _| {
            ANSWERS.lock().unwrap().push(format!("{result:?}"));
        });
    }

    fn wait(what: &str, limit: Duration, f: impl Fn() -> bool) -> bool {
        let start = rune::time::now();
        while !f() {
            if start.elapsed() > limit { println!("share_macos: no {what} within {limit:?}"); return false; }
            std::thread::sleep(Duration::from_millis(5));
        }
        true
    }

    fn on_main<T: Send + 'static>(handle: &AppHandle<Main>, f: impl FnOnce(&mut Main, &mut Tx) -> T + Send + 'static) -> T {
        let (tx, rx) = mpsc::channel();
        handle.update(move |s, t| { tx.send(f(s, t)).ok(); });
        rx.recv_timeout(Duration::from_secs(5)).expect("an update runs on the main thread")
    }

    unsafe fn windows() -> Vec<usize> {
        // SAFETY: caller invokes on the AppKit main thread; reads `NSApp.windows`.
        unsafe {
            let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
            let windows: *mut AnyObject = msg_send![app, windows];
            let count: usize = msg_send![windows, count];
            (0..count).map(|i| { let w: *mut AnyObject = msg_send![windows, objectAtIndex: i]; w as usize }).collect()
        }
    }

    fn picker_windows(handle: &AppHandle<Main>) -> usize {
        on_main(handle, |_, _| {
            let baseline = BASELINE.with(|b| b.borrow().clone());
            // SAFETY: main thread (an update); each address is a live window of `NSApp.windows`.
            unsafe {
                windows().into_iter().filter(|w| !baseline.contains(w)).filter(|&w| {
                    let visible: bool = msg_send![w as *mut AnyObject, isVisible];
                    visible
                }).count()
            }
        })
    }

    fn observe_menus() {
        // SAFETY: main thread; the observer tokens are kept for the process.
        unsafe {
            let center: *mut AnyObject = msg_send![class!(NSNotificationCenter), defaultCenter];
            for (name, begin) in [("NSMenuDidBeginTrackingNotification", true), ("NSMenuDidEndTrackingNotification", false)] {
                let block = block2::RcBlock::new(move |note: *mut AnyObject| {
                    let menu: *mut AnyObject = msg_send![note, object];
                    MENU.with(|slot| *slot.borrow_mut() = if begin { Retained::retain(menu) } else { None });
                    TRACKING.store(begin, Ordering::SeqCst);
                });
                let token: Option<Retained<AnyObject>> = msg_send![center, addObserverForName: &*NSString::from_str(name),
                    object: std::ptr::null::<AnyObject>(), queue: std::ptr::null::<AnyObject>(), usingBlock: &*block];
                OBSERVERS.with(|o| o.borrow_mut().extend(token));
            }
        }
    }

    fn click_outside(handle: &AppHandle<Main>) {
        on_main(handle, move |_, _| {
            let baseline = BASELINE.with(|b| b.borrow().clone());
            // SAFETY: main thread (an update); the tracking NSMenu, the app's live windows, and
            // autoreleased NSEvents that postEvent:atStart: retains.
            unsafe {
                if let Some(menu) = MENU.with(|slot| slot.borrow().clone()) {
                    println!("share_macos: the picker tracks as a menu; cancelling its tracking through AppKit");
                    let _: () = msg_send![&*menu, cancelTracking];
                    return;
                }
                for w in windows().into_iter().filter(|w| !baseline.contains(w)) {
                    let class: *const AnyObject = msg_send![w as *mut AnyObject, className];
                    println!("share_macos: picker window class {}", *(class as *const NSString));
                }
                let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
                let Some(&rune_window) = baseline.iter().find(|&&w| {
                    let class: *const AnyObject = msg_send![w as *mut AnyObject, className];
                    (*(class as *const NSString)).to_string().starts_with("Winit")
                }) else { panic!("no winit window to click") };
                let number: isize = msg_send![rune_window as *mut AnyObject, windowNumber];
                for kind in [1usize, 2] {
                    let info: *mut AnyObject = msg_send![class!(NSProcessInfo), processInfo];
                    let now: f64 = msg_send![info, systemUptime];
                    let event: *mut AnyObject = msg_send![class!(NSEvent), mouseEventWithType: kind, location: objc2_foundation::NSPoint::new(8.0, 8.0),
                        modifierFlags: 0usize, timestamp: now, windowNumber: number, context: std::ptr::null::<AnyObject>(),
                        eventNumber: 0isize, clickCount: 1isize, pressure: if kind == 1 { 1.0f32 } else { 0.0f32 }];
                    assert!(!event.is_null(), "NSEvent made the click");
                    let _: () = msg_send![app, postEvent: event, atStart: false];
                }
            }
        });
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGPreflightPostEventAccess() -> bool;
        fn CGEventCreateKeyboardEvent(source: *const c_void, key: u16, down: bool) -> *mut c_void;
        fn CGEventPostToPid(pid: i32, event: *mut c_void);
        fn CFRelease(object: *mut c_void);
    }

    fn real_escape_key() {
        for down in [true, false] {
            // SAFETY: a null source makes an event from the default state; 53 is Escape's key
            // code. The event goes to this process only, then is released once.
            unsafe {
                let event = CGEventCreateKeyboardEvent(std::ptr::null(), 53, down);
                assert!(!event.is_null(), "CoreGraphics made the Escape key event");
                CGEventPostToPid(std::process::id() as i32, event);
                CFRelease(event);
            }
        }
    }

    fn key_window(handle: &AppHandle<Main>) -> String {
        on_main(handle, |_, _| {
            // SAFETY: main thread (an update); `keyWindow` may be nil and is checked.
            unsafe {
                let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
                let key: *mut AnyObject = msg_send![app, keyWindow];
                if key.is_null() { return "none".to_owned(); }
                let class: *const AnyObject = msg_send![key, className];
                (*(class as *const NSString)).to_string()
            }
        })
    }

    fn activate(handle: &AppHandle<Main>) -> bool {
        on_main(handle, |_, _| {
            // SAFETY: main thread (an update); activates this app and reads the frontmost one,
            // checked non-null.
            unsafe {
                let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
                let _: () = msg_send![app, activateIgnoringOtherApps: true];
                let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
                let front: *mut AnyObject = msg_send![workspace, frontmostApplication];
                !front.is_null() && { let pid: i32 = msg_send![front, processIdentifier]; pid as u32 == std::process::id() }
            }
        })
    }

    fn make_picker_key(handle: &AppHandle<Main>) -> bool {
        on_main(handle, |_, _| {
            let baseline = BASELINE.with(|b| b.borrow().clone());
            // SAFETY: main thread (an update); a live, visible window of the app.
            unsafe {
                let Some(picker) = windows().into_iter().filter(|w| !baseline.contains(w)).find(|&w| {
                    let visible: bool = msg_send![w as *mut AnyObject, isVisible];
                    visible
                }) else { return false };
                let _: () = msg_send![picker as *mut AnyObject, makeKeyWindow];
                true
            }
        })
    }

    fn escape(handle: &AppHandle<Main>, when: &str) -> bool {
        let before = answers().len();
        println!("share_macos: real Escape ({when}): key window {}", key_window(handle));
        real_escape_key();
        let answered = wait("an answer to Escape", Duration::from_secs(2), || answers().len() > before);
        println!("share_macos: real Escape ({when}): answered {answered}, {} picker window(s) on screen, key window now {}",
            picker_windows(handle), key_window(handle));
        answered
    }

    fn real_escape(handle: &AppHandle<Main>) {
        // SAFETY: takes no arguments and has no preconditions.
        if !unsafe { CGPreflightPostEventAccess() } {
            println!("share_macos: no permission to post system events (Accessibility); skipping the real Escape");
            return;
        }
        let frontmost = activate(handle);
        std::thread::sleep(Duration::from_millis(300));
        open_picker(handle);
        std::thread::sleep(Duration::from_millis(300));
        println!("share_macos: real Escape: this app frontmost {frontmost}");
        let count = answers().len();
        if !escape(handle, "Rune's window key") {
            if make_picker_key(handle) {
                std::thread::sleep(Duration::from_millis(300));
                escape(handle, "the picker's window made key through AppKit");
            } else {
                println!("share_macos: no picker window to make key");
            }
            if answers().len() == count { click_outside(handle); }
        }
        assert!(wait("the picker's answer", Duration::from_secs(3), || answers().len() == count + 1), "the picker never answered");
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(answers()[count..], ["Ok(None)"], "the picker's dismissal answered once with none: {:?}", answers());
    }

    fn open_picker(handle: &AppHandle<Main>) {
        handle.update(share);
        assert!(wait("the picker", Duration::from_secs(5), || TRACKING.load(Ordering::SeqCst) || picker_windows(handle) > 0), "the share picker never appeared");
    }

    fn answers() -> Vec<String> { ANSWERS.lock().unwrap().clone() }

    pub fn main() {
        if std::env::var_os("RUNE_GUI_TESTS").is_none() {
            println!("share_macos: skipped (set RUNE_GUI_TESTS=1 in a GUI session)");
            return;
        }
        std::thread::spawn(|| { std::thread::sleep(Duration::from_secs(90)); eprintln!("timeout"); std::process::exit(2) });
        rune::orderly_exit();
        let spec = AppSpec { poll: Some(main_poll), ..AppSpec::new("share", (320.0, 160.0), || Main, main_view) };
        let (ready_tx, ready_rx) = mpsc::channel();
        let mut app = App::new(spec).args(Vec::new()).on_ready(move |_| { ready_tx.send(()).ok(); });
        let handle = app.handle();
        let second = App::new(AppSpec { poll: Some(second_poll), ..AppSpec::new("share second", (200.0, 120.0), || Second, second_view) });
        let (second, _) = app.window(WindowSpec::from_app("second", second));
        let worker = std::thread::spawn(move || {
            let run = std::panic::catch_unwind(|| {
                ready_rx.recv_timeout(Duration::from_secs(60)).expect("window ready");
                assert!(wait("the second window", Duration::from_secs(5), || second.is_visible()), "the second window never showed");
                std::thread::sleep(Duration::from_millis(300));
                run(&handle);
            });
            if run.is_err() { eprintln!("share_macos: failed"); std::process::exit(1); }
            handle.quit();
            run.is_ok()
        });
        app.run();
        assert!(worker.join().unwrap(), "share_macos failed");
        println!("share_macos: ok");
    }

    fn run(handle: &AppHandle<Main>) {
        on_main(handle, |_, _| {
            observe_menus();
            // SAFETY: main thread (an update).
            let now = unsafe { windows() };
            BASELINE.with(|b| *b.borrow_mut() = now);
        });
        let before = SECOND_FRAMES.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(500));
        println!("share_macos: second window drew {} frames in 500 ms before the picker", SECOND_FRAMES.load(Ordering::SeqCst) - before);

        open_picker(handle);
        println!("share_macos: the picker opened as {}", if TRACKING.load(Ordering::SeqCst) { "a tracking menu" } else { "a window" });

        let (main_before, second_before) = (MAIN_FRAMES.load(Ordering::SeqCst), SECOND_FRAMES.load(Ordering::SeqCst));
        std::thread::sleep(Duration::from_millis(500));
        let main_during = MAIN_FRAMES.load(Ordering::SeqCst) - main_before;
        let second_during = SECOND_FRAMES.load(Ordering::SeqCst) - second_before;
        println!("share_macos: {main_during} main and {second_during} second-window frames in 500 ms while the picker was open");
        assert!(second_during >= 10, "the second window stalled while the picker was open ({second_during} frames in 500 ms)");
        assert!(main_during >= 10, "the sharing window stalled while the picker was open ({main_during} frames in 500 ms)");
        assert!(answers().is_empty(), "nothing answers while the picker is open: {:?}", answers());

        click_outside(handle);
        assert!(wait("the dismissal's answer", Duration::from_secs(3), || !answers().is_empty()), "a click outside the picker never answered");
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(answers(), ["Ok(None)"], "the dismissal reaches the callback once");
        assert!(wait("the picker to leave the screen", Duration::from_secs(2), || !TRACKING.load(Ordering::SeqCst) && picker_windows(handle) == 0), "the picker stayed on screen");
        let before = SECOND_FRAMES.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(200));
        assert!(SECOND_FRAMES.load(Ordering::SeqCst) > before, "frames keep running after the picker");

        open_picker(handle);
        click_outside(handle);
        assert!(wait("the second answer", Duration::from_secs(3), || answers().len() == 2), "the second picker's dismissal never answered");
        assert_eq!(answers(), ["Ok(None)", "Ok(None)"]);

        open_picker(handle);
        handle.hide();
        assert!(wait("the hide's answer", Duration::from_secs(3), || answers().len() == 3), "hiding the window under the picker never answered");
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(answers(), ["Ok(None)", "Ok(None)", "Ok(None)"], "the hide answers once, and the picker's own answer after it doesn't");
        println!("share_macos: {} picker window(s) on screen after the hide", picker_windows(handle));
        handle.open();
        assert!(wait("the window back", Duration::from_secs(3), || handle.is_visible()));
        std::thread::sleep(Duration::from_millis(300));

        real_escape(handle);
    }
}
