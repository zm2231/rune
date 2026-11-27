//! The share picker opens without stopping Rune: while it is on screen, a spinner in a second
//! window keeps drawing, and the picker's dismissal reaches the callback exactly once. It
//! dismisses the picker as a user would (Escape, then a click outside it), and closes its window
//! where neither does, printing what each step left on screen. Opens windows, so it only runs
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

    #[derive(Clone, Copy, Debug)]
    enum Dismiss { Escape, ClickOutside, CloseWindow }

    fn dismiss(handle: &AppHandle<Main>, how: Dismiss) {
        on_main(handle, move |_, _| {
            let baseline = BASELINE.with(|b| b.borrow().clone());
            // SAFETY: main thread (an update); the tracking NSMenu, the app's live windows, and an
            // autoreleased NSEvent that postEvent:atStart: retains.
            unsafe {
                if let Some(menu) = MENU.with(|slot| slot.borrow().clone()) {
                    let _: () = msg_send![&*menu, cancelTracking];
                    return;
                }
                let added: Vec<usize> = windows().into_iter().filter(|w| !baseline.contains(w)).filter(|&w| {
                    let visible: bool = msg_send![w as *mut AnyObject, isVisible];
                    visible
                }).collect();
                for &w in &added {
                    let class: *const AnyObject = msg_send![w as *mut AnyObject, className];
                    let class = &*(class as *const NSString);
                    let parent: *mut AnyObject = msg_send![w as *mut AnyObject, parentWindow];
                    let sheet_parent: *mut AnyObject = msg_send![w as *mut AnyObject, sheetParent];
                    let level: isize = msg_send![w as *mut AnyObject, level];
                    let controller: *mut AnyObject = msg_send![w as *mut AnyObject, contentViewController];
                    let controller = if controller.is_null() { "none".to_owned() } else {
                        let name: *const AnyObject = msg_send![controller, className];
                        (*(name as *const NSString)).to_string()
                    };
                    println!("share_macos: picker window class {class}, child of a Rune window {}, sheet {}, level {level}, controller {controller}",
                        baseline.contains(&(parent as usize)), !sheet_parent.is_null());
                }
                match how {
                    Dismiss::Escape => {
                        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
                        let chars = NSString::from_str("\u{1b}");
                        for &w in &added {
                            let number: isize = msg_send![w as *mut AnyObject, windowNumber];
                            for kind in [10usize, 11] {
                                let info: *mut AnyObject = msg_send![class!(NSProcessInfo), processInfo];
                                let now: f64 = msg_send![info, systemUptime];
                                let event: *mut AnyObject = msg_send![class!(NSEvent), keyEventWithType: kind, location: objc2_foundation::NSPoint::new(0.0, 0.0),
                                    modifierFlags: 0usize, timestamp: now, windowNumber: number, context: std::ptr::null::<AnyObject>(),
                                    characters: &*chars, charactersIgnoringModifiers: &*chars, isARepeat: false, keyCode: 53u16];
                                assert!(!event.is_null(), "NSEvent made the Escape key event");
                                let _: () = msg_send![app, postEvent: event, atStart: false];
                            }
                        }
                    }
                    Dismiss::ClickOutside => {
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
                    Dismiss::CloseWindow => for &w in &added { let _: () = msg_send![w as *mut AnyObject, close]; },
                }
            }
        });
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

        handle.update(share);
        let opened = wait("the picker", Duration::from_secs(5), || TRACKING.load(Ordering::SeqCst) || picker_windows(handle) > 0);
        assert!(opened, "the share picker never appeared");
        println!("share_macos: the picker opened as {}", if TRACKING.load(Ordering::SeqCst) { "a tracking menu" } else { "a window" });

        let (main_before, second_before) = (MAIN_FRAMES.load(Ordering::SeqCst), SECOND_FRAMES.load(Ordering::SeqCst));
        std::thread::sleep(Duration::from_millis(500));
        let main_during = MAIN_FRAMES.load(Ordering::SeqCst) - main_before;
        let second_during = SECOND_FRAMES.load(Ordering::SeqCst) - second_before;
        println!("share_macos: {main_during} main and {second_during} second-window frames in 500 ms while the picker was open");
        assert!(second_during >= 10, "the second window stalled while the picker was open ({second_during} frames in 500 ms)");
        assert!(main_during >= 10, "the sharing window stalled while the picker was open ({main_during} frames in 500 ms)");
        assert!(answers().is_empty(), "nothing answers while the picker is open: {:?}", answers());

        let mut answered_by = None;
        for how in [Dismiss::Escape, Dismiss::ClickOutside, Dismiss::CloseWindow] {
            dismiss(handle, how);
            let answered = wait("an answer", Duration::from_secs(3), || !answers().is_empty());
            println!("share_macos: after {how:?}: answered {answered}, {} picker window(s) on screen", picker_windows(handle));
            if answered { answered_by = Some(how); break; }
        }
        let Some(how) = answered_by else { panic!("the dismissal never answered") };
        println!("share_macos: the first picker answered after {how:?}");
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(answers(), ["Ok(None)"], "the dismissal reaches the callback once");
        let before = SECOND_FRAMES.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(200));
        assert!(SECOND_FRAMES.load(Ordering::SeqCst) > before, "frames keep running after the picker");

        handle.update(share);
        assert!(wait("the picker again", Duration::from_secs(5), || TRACKING.load(Ordering::SeqCst) || picker_windows(handle) > 0), "the picker didn't open a second time");
        dismiss(handle, Dismiss::CloseWindow);
        let answered = wait("the second answer", Duration::from_secs(5), || answers().len() == 2);
        println!("share_macos: after closing the second picker's window: answered {answered}, {} picker window(s) on screen", picker_windows(handle));
        assert!(answered, "closing the picker's window never answered");
        assert_eq!(answers(), ["Ok(None)", "Ok(None)"]);
    }
}
