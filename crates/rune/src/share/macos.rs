//! The share sheet on macOS: an `NSSharingServicePicker` beside the anchor in the window's
//! content view, which macOS shows as a popover. The picker answers when the user chooses a
//! service, with its title, or dismisses the popover (a click outside it), with none. It opens
//! from a run-loop block after the winit handler returns (`platform::macos::run_later`), so every
//! window keeps drawing while it shows. Escape closes it, with none: the popover never becomes key,
//! so a local event monitor, installed while a picker shows, takes Escape in the window under it
//! before AppKit dispatches it, and neither that press, its repeats nor its release reach the app.
//! Should `close` not answer, the share answers none anyway, and the popover's window is closed
//! when it is the only picker's. A window hidden or closed under it answers its share itself
//! (`RuntimeCore::cancel_shares`), which then closes and forgets that window's pickers.
//! `RUNE_SHARE_LOG=1` logs each step to stderr.

#![allow(unsafe_code)]

use std::cell::RefCell;

use block2::RcBlock;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, NSObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, class, define_class, msg_send, sel};
use objc2_app_kit::{NSApplication, NSWindow};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{NSString, NSURL};

use super::{Reply, Shown};

/// `NSRectEdgeMinY`: below the anchor in a flipped view, above it otherwise.
const MIN_Y_EDGE: usize = 1;
const KEY_DOWN_AND_UP: usize = (1 << 10) | (1 << 11);
const KEY_DOWN: usize = 10;
const KEY_UP: usize = 11;
const ESCAPE: u16 = 53;

fn log() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("RUNE_SHARE_LOG").is_some_and(|v| v != "0"))
}

define_class!(
    // SAFETY: `NSObject` has no subclassing requirements, and the class has no `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RuneSharePickerDelegate"]
    #[ivars = RefCell<Option<Reply>>]
    struct Delegate;

    impl Delegate {
        #[unsafe(method(sharingServicePicker:didChooseSharingService:))]
        fn chose(&self, _picker: &AnyObject, service: Option<&AnyObject>) {
            let title = service.map(|service| {
                // SAFETY: the picker passes an `NSSharingService`, whose `title` takes no
                // arguments and returns a non-null `NSString`, retained here.
                let title: Retained<NSString> = unsafe { msg_send![service, title] };
                title.to_string()
            });
            if log() { eprintln!("rune: share picker delegate answered {title:?}"); }
            self.answer(title);
        }
    }
);

impl Delegate {
    fn answer(&self, title: Option<String>) {
        if let Some(reply) = self.ivars().borrow_mut().take() { reply.send(Ok(title)); }
        let answered: *const Delegate = self;
        crate::platform::macos::run_later("share picker", move || {
            SHOWING.with(|showing| showing.borrow_mut().retain(|s| !std::ptr::eq(&*s.delegate, answered)));
            unwatch_if_idle();
        });
    }

    fn open(&self) -> bool { self.ivars().borrow().is_some() }
}

#[derive(Clone)]
struct Showing {
    picker: Retained<AnyObject>,
    delegate: Retained<Delegate>,
    host: Retained<NSWindow>,
    before: Vec<Retained<NSWindow>>,
}

thread_local! {
    static SHOWING: RefCell<Vec<Showing>> = const { RefCell::new(Vec::new()) };
    static ESCAPED: RefCell<Vec<Retained<NSWindow>>> = const { RefCell::new(Vec::new()) };
    static MONITOR: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
}

fn watch_escape() {
    if MONITOR.with(|m| m.borrow().is_some()) { return; }
    let handler = RcBlock::new(|event: *mut AnyObject| -> *mut AnyObject {
        // SAFETY: AppKit passes a live key event on the main thread; `type`, `keyCode`, `isARepeat`
        // and `window` are its plain getters.
        let (kind, code, repeat, window) = unsafe {
            let kind: usize = msg_send![event, type];
            let code: u16 = msg_send![event, keyCode];
            let repeat: bool = if kind == KEY_DOWN { msg_send![event, isARepeat] } else { false };
            let window: *mut NSWindow = msg_send![event, window];
            (kind, code, repeat, window)
        };
        if code != ESCAPE || !(kind == KEY_DOWN || kind == KEY_UP) { return event; }
        // SAFETY: `window` is the live event's window or null; retaining it keeps it for the hold.
        let Some(window) = (unsafe { Retained::retain(window) }) else { return event };
        if escape(window, kind == KEY_DOWN, repeat) { std::ptr::null_mut() } else { event }
    });
    // SAFETY: the monitor copies the handler; `unwatch_if_idle` removes it with its token.
    let token: Option<Retained<AnyObject>> = unsafe {
        msg_send![class!(NSEvent), addLocalMonitorForEventsMatchingMask: KEY_DOWN_AND_UP, handler: &*handler]
    };
    if log() { eprintln!("rune: share picker Escape monitor installed: {}", token.is_some()); }
    MONITOR.with(|m| *m.borrow_mut() = token);
}

fn unwatch_if_idle() {
    let idle = SHOWING.with(|s| s.borrow().is_empty()) && ESCAPED.with(|e| e.borrow().is_empty());
    if !idle { return; }
    if let Some(token) = MONITOR.with(|m| m.borrow_mut().take()) {
        // SAFETY: the token came from addLocalMonitorForEventsMatchingMask.
        unsafe { let _: () = msg_send![class!(NSEvent), removeMonitor: &*token]; }
    }
}

fn class_name(object: &NSWindow) -> String {
    let object: &AnyObject = object;
    object.class().name().to_string_lossy().into_owned()
}

/// Closes and forgets, a run-loop pass later, the pickers over windows no longer on screen, whose
/// shares their runtimes have answered (`RuntimeCore::cancel_shares`).
pub(crate) fn forget_hidden() {
    crate::platform::macos::run_later("share picker", || {
        let gone: Vec<Showing> = SHOWING.with(|showing| {
            let mut showing = showing.borrow_mut();
            let (gone, kept) = std::mem::take(&mut *showing).into_iter().partition(|s| !s.host.isVisible());
            *showing = kept;
            gone
        });
        ESCAPED.with(|escaped| escaped.borrow_mut().retain(|w| w.isVisible()));
        for Showing { picker, delegate, .. } in gone {
            if delegate.open() {
                // SAFETY: as in `escape`.
                unsafe {
                    let closes: bool = msg_send![&*picker, respondsToSelector: sel!(close)];
                    if closes { let _: () = msg_send![&*picker, close]; }
                }
            }
            drop(delegate.ivars().borrow_mut().take());
        }
        unwatch_if_idle();
    });
}

fn escape(window: Retained<NSWindow>, pressed: bool, repeat: bool) -> bool {
    let mine = |w: &NSWindow| std::ptr::eq(w, &*window);
    let held = ESCAPED.with(|escaped| {
        let mut escaped = escaped.borrow_mut();
        let held = escaped.iter().any(|w| mine(w));
        if !pressed || !repeat { escaped.retain(|w| !mine(w)); }
        held
    });
    if !pressed || repeat {
        if !pressed && held { crate::platform::macos::run_later("share picker", unwatch_if_idle); }
        return held;
    }
    let open: Vec<Showing> = SHOWING.with(|showing| showing.borrow().iter()
        .filter(|s| mine(&s.host) && s.delegate.open()).cloned().collect());
    if log() { eprintln!("rune: share picker Escape keyDown in window {}: {} open picker(s) over it", class_name(&window), open.len()); }
    if open.is_empty() { return false; }
    for Showing { picker, delegate, before, .. } in open {
        // SAFETY: `respondsToSelector:` takes a selector and returns a `BOOL`; `close` takes no
        // arguments and answers the delegate with no service.
        let closes = unsafe {
            let closes: bool = msg_send![&*picker, respondsToSelector: sel!(close)];
            if closes { let _: () = msg_send![&*picker, close]; }
            closes
        };
        if log() { eprintln!("rune: share picker close {}; answered at once: {}", if closes { "sent" } else { "unavailable" }, !delegate.open()); }
        crate::platform::macos::run_later("share picker", move || {
            if !delegate.open() { return; }
            let Some(mtm) = MainThreadMarker::new() else { return };
            let alone = SHOWING.with(|showing| showing.borrow().iter().filter(|s| s.delegate.open()).count() == 1);
            let popovers: Vec<Retained<NSWindow>> = NSApplication::sharedApplication(mtm).windows().iter()
                .filter(|w| w.isVisible() && !before.iter().any(|b| std::ptr::eq(&**b, &**w)) && class_name(w).contains("Popover"))
                .collect();
            let close = alone && popovers.len() == 1;
            if log() { eprintln!("rune: share picker still open after close; {} popover window(s) new since it showed; closing it: {close}; answering none", popovers.len()); }
            if close { popovers[0].close(); }
            delegate.answer(None);
        });
    }
    ESCAPED.with(|escaped| escaped.borrow_mut().push(window));
    true
}

/// Shows `shown` over `window`'s content view once the current winit handler returns. A
/// window hidden or closed by then answers `None`.
pub(crate) fn show(shown: Shown, window: &winit::window::Window) {
    let Shown { share, anchor, reply } = shown;
    let (Some(mtm), Some(view)) = (MainThreadMarker::new(), crate::platform::macos::content_view(window)) else {
        reply.send(Err(super::ShareError::Failed("the window has no view to show the picker in".into())));
        return;
    };
    crate::platform::macos::run_later("share picker", move || {
        let Some(host) = view.window().filter(|w| w.isVisible()) else {
            reply.send(Ok(None));
            return;
        };
        // SAFETY: `init` is `NSObject`'s designated initializer, sent to an allocated `Delegate`
        // whose ivars are set.
        let delegate: Retained<Delegate> = unsafe { msg_send![super(mtm.alloc::<Delegate>().set_ivars(RefCell::new(Some(reply)))), init] };
        // SAFETY: an array of `NSString` and `NSURL` items makes a picker; its delegate is an
        // object that answers the delegate method above.
        let picker = unsafe {
            let items: Retained<AnyObject> = msg_send![class!(NSMutableArray), array];
            let add = |item: &AnyObject| { let _: () = msg_send![&*items, addObject: item]; };
            let text = [share.title.as_deref(), share.text.as_deref()].into_iter().flatten().filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n");
            if !text.is_empty() { add(&NSString::from_str(&text)); }
            if let Some(url) = share.url.as_deref().and_then(|url| NSURL::URLWithString(&NSString::from_str(url))) { add(&url); }
            for file in share.files.iter().filter_map(|f| f.to_str()) { add(&NSURL::fileURLWithPath(&NSString::from_str(file))); }
            let picker: Allocated<AnyObject> = msg_send![class!(NSSharingServicePicker), alloc];
            let picker: Retained<AnyObject> = msg_send![picker, initWithItems: &*items];
            let _: () = msg_send![&*picker, setDelegate: &*delegate];
            picker
        };
        let before = NSApplication::sharedApplication(mtm).windows().iter().collect();
        SHOWING.with(|showing| {
            let mut showing = showing.borrow_mut();
            showing.retain(|s| s.host.isVisible());
            showing.push(Showing { picker: picker.clone(), delegate, host, before });
        });
        watch_escape();
        // SAFETY: `bounds` is a rect and `isFlipped` a `BOOL` of the view, and the picker shows
        // relative to a rect in it.
        unsafe {
            let bounds: CGRect = msg_send![&*view, bounds];
            let flipped: bool = msg_send![&*view, isFlipped];
            let [x, y, w, h] = anchor.unwrap_or([bounds.size.width as f32 / 2.0, bounds.size.height as f32 / 2.0, 0.0, 0.0]).map(f64::from);
            let y = if flipped { y } else { bounds.size.height - y - h };
            let rect = CGRect::new(CGPoint::new(x, y), CGSize::new(w.max(1.0), h.max(1.0)));
            let _: () = msg_send![&*picker, showRelativeToRect: rect, ofView: &*view, preferredEdge: MIN_Y_EDGE];
        }
    });
}
