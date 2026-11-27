#![allow(unsafe_code)]

use std::cell::{Cell, RefCell};
use std::ffi::{c_char, c_void};

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, NSObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_foundation::{NSString, NSURL};

use super::QuickLook;

const KEY_DOWN: usize = 10;

define_class!(
    // SAFETY: `NSObject` has no subclassing requirements, and the class has no `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RuneQuickLookSource"]
    struct Source;

    impl Source {
        #[unsafe(method(numberOfPreviewItemsInPreviewPanel:))]
        fn count(&self, _panel: &AnyObject) -> isize {
            PANEL.with(|p| p.borrow().items.len() as isize)
        }

        #[unsafe(method_id(previewPanel:previewItemAtIndex:))]
        fn item(&self, _panel: &AnyObject, index: isize) -> Option<Retained<NSURL>> {
            PANEL.with(|p| p.borrow().items.get(usize::try_from(index).ok()?).cloned())
        }

        #[unsafe(method(previewPanel:handleEvent:))]
        fn handle_event(&self, panel: &AnyObject, event: &AnyObject) -> Bool {
            // SAFETY: `event` is an `NSEvent`: its `type` is an `NSUInteger`, and a key event's
            // `charactersIgnoringModifiers` an `NSString`. `close` is an `NSWindow` method of
            // the panel.
            unsafe {
                let kind: usize = msg_send![event, type];
                if kind != KEY_DOWN { return Bool::NO; }
                let chars: Option<Retained<NSString>> = msg_send![event, charactersIgnoringModifiers];
                if chars.is_none_or(|c| c.to_string() != " ") { return Bool::NO; }
                let _: () = msg_send![panel, close];
            }
            Bool::YES
        }

        #[unsafe(method(windowWillClose:))]
        fn will_close(&self, _notification: &AnyObject) {
            closed();
        }
    }
);

#[derive(Default)]
struct Panel {
    items: Vec<Retained<NSURL>>,
    index: usize,
    source: Option<Retained<Source>>,
    on_closed: Option<Box<dyn Fn()>>,
}

thread_local! {
    static PANEL: RefCell<Panel> = RefCell::default();
    static HOOKED: Cell<*const AnyClass> = const { Cell::new(std::ptr::null()) };
}

unsafe extern "C" {
    fn class_addMethod(class: *const AnyClass, selector: Sel, implementation: *const c_void, types: *const c_char) -> Bool;
}

fn closed() {
    let on_closed = PANEL.with(|p| p.try_borrow_mut().ok().and_then(|mut p| p.on_closed.take()));
    if let Some(f) = on_closed { f(); }
}

fn panel_class() -> Option<&'static AnyClass> {
    if let Some(class) = AnyClass::get(c"QLPreviewPanel") { return Some(class); }
    let path = NSString::from_str("/System/Library/Frameworks/Quartz.framework");
    // SAFETY: `bundleWithPath:` answers an `NSBundle` or nil, and `load` a `BOOL`.
    unsafe {
        let bundle: Option<Retained<AnyObject>> = msg_send![objc2::class!(NSBundle), bundleWithPath: &*path];
        let _: Bool = msg_send![&*bundle?, load];
    }
    AnyClass::get(c"QLPreviewPanel")
}

fn shared_panel(class: &AnyClass) -> Retained<AnyObject> {
    // SAFETY: `sharedPreviewPanel` answers the app's one `QLPreviewPanel`.
    unsafe { msg_send![class, sharedPreviewPanel] }
}

extern "C" fn accepts(_: *mut AnyObject, _: Sel, _: *mut AnyObject) -> Bool {
    Bool::new(PANEL.with(|p| p.try_borrow().is_ok_and(|p| !p.items.is_empty())))
}

extern "C" fn begin(_: *mut AnyObject, _: Sel, panel: *mut AnyObject) {
    // SAFETY: AppKit hands the controller the `QLPreviewPanel` it is about to control.
    let Some(panel) = (unsafe { panel.as_ref() }) else { return };
    let Some(mtm) = MainThreadMarker::new() else { return };
    let Some((source, index)) = PANEL.with(|p| {
        let mut p = p.try_borrow_mut().ok()?;
        let source = p.source.get_or_insert_with(|| {
            // SAFETY: `init` is `NSObject`'s designated initializer.
            unsafe { msg_send![mtm.alloc::<Source>(), init] }
        }).clone();
        Some((source, p.index))
    }) else { return };
    // SAFETY: the panel's data source answers the two data source methods and its delegate
    // `previewPanel:handleEvent:` and `windowWillClose:`; the panel holds both weakly, and
    // `PANEL` keeps the source alive. `currentPreviewItemIndex` is an `NSInteger`.
    unsafe {
        let _: () = msg_send![panel, setDataSource: &*source];
        let _: () = msg_send![panel, setDelegate: &*source];
        let _: () = msg_send![panel, reloadData];
        let _: () = msg_send![panel, setCurrentPreviewItemIndex: index as isize];
    }
}

extern "C" fn end(_: *mut AnyObject, _: Sel, panel: *mut AnyObject) {
    // SAFETY: AppKit hands the controller the `QLPreviewPanel` it controlled.
    let Some(panel) = (unsafe { panel.as_ref() }) else { return };
    // SAFETY: clearing the panel's data source and delegate; `isVisible` is a `BOOL`.
    let visible = unsafe {
        let _: () = msg_send![panel, setDataSource: None::<&AnyObject>];
        let _: () = msg_send![panel, setDelegate: None::<&AnyObject>];
        let visible: Bool = msg_send![panel, isVisible];
        visible.as_bool()
    };
    if !visible { closed(); }
}

fn hook(view: &AnyObject) {
    let class = view.class();
    if HOOKED.with(|h| std::ptr::eq(h.get(), class)) { return; }
    let (b, v) = if cfg!(target_arch = "aarch64") { (c"B@:@", c"v@:@") } else { (c"c@:@", c"v@:@") };
    let methods: [(Sel, *const c_void, &std::ffi::CStr); 3] = [
        (sel!(acceptsPreviewPanelControl:), accepts as extern "C" fn(_, _, _) -> _ as *const c_void, b),
        (sel!(beginPreviewPanelControl:), begin as extern "C" fn(_, _, _) as *const c_void, v),
        (sel!(endPreviewPanelControl:), end as extern "C" fn(_, _, _) as *const c_void, v),
    ];
    for (selector, imp, types) in methods {
        // SAFETY: each function has its selector's ABI as `types` spells it (BOOL is `bool` on
        // Apple silicon, `signed char` on Intel), and adding a method the view's class lacks
        // leaves its other methods alone. A class that already answers keeps its own.
        if !unsafe { class_addMethod(class, selector, imp, types.as_ptr()) }.as_bool() {
            eprintln!("rune: the window's view already answers {selector:?}; Quick Look stays with it");
        }
    }
    HOOKED.with(|h| h.set(class));
}

pub(super) fn show(look: Option<QuickLook>, window: &winit::window::Window, on_closed: Box<dyn Fn()>) {
    let (Some(_), Some(view), Some(class)) = (MainThreadMarker::new(), crate::platform::macos::content_view(window), panel_class()) else {
        on_closed();
        return;
    };
    hook(&view);
    let panel = shared_panel(class);
    // SAFETY: `isVisible` is a `BOOL` of the panel.
    let visible: Bool = unsafe { msg_send![&*panel, isVisible] };
    let visible = visible.as_bool();
    let Some(look) = look else {
        PANEL.with(|p| {
            let mut p = p.borrow_mut();
            p.items.clear();
            p.on_closed = None;
        });
        // SAFETY: `close` is an `NSWindow` method of the panel.
        if visible { unsafe { let _: () = msg_send![&*panel, close]; } }
        return;
    };
    let index = look.index();
    PANEL.with(|p| {
        let mut p = p.borrow_mut();
        p.items = look.files.iter().filter_map(|f| f.to_str()).map(|f| NSURL::fileURLWithPath(&NSString::from_str(f))).collect();
        p.index = index;
        p.on_closed = Some(on_closed);
    });
    // SAFETY: `QLPreviewPanel` methods on the shared panel, on the main thread. Showing it asks
    // the key window's responder chain, where the hooked view answers, for a controller.
    unsafe {
        if visible {
            let _: () = msg_send![&*panel, updateController];
            let _: () = msg_send![&*panel, reloadData];
            let _: () = msg_send![&*panel, setCurrentPreviewItemIndex: index as isize];
        } else {
            let _: () = msg_send![&*panel, makeKeyAndOrderFront: None::<&AnyObject>];
        }
    }
}
