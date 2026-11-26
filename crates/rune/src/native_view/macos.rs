#![allow(unsafe_code)]

use std::ffi::{CStr, c_void};
use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
use objc2::{msg_send, sel};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use winit::raw_window_handle::{AppKitWindowHandle, HandleError, RawWindowHandle, WindowHandle};

use super::clips::{ClipView, Clips, Placement};
use super::{NativeViewDraw, NativeViewId};

const NS_WINDOW_ABOVE: isize = 1;
const WIDTH_HEIGHT_SIZABLE: usize = 18;

fn rect(r: [f64; 4]) -> CGRect {
    CGRect { origin: CGPoint { x: r[0], y: r[1] }, size: CGSize { width: r[2], height: r[3] } }
}

unsafe extern "C-unwind" fn flipped(_: &AnyObject, _: Sel) -> Bool { Bool::YES }

fn class(name: &'static CStr, build: impl FnOnce(&mut ClassBuilder)) -> Option<&'static AnyClass> {
    if let Some(class) = AnyClass::get(name) { return Some(class); }
    let mut builder = ClassBuilder::new(name, AnyClass::get(c"NSView")?)?;
    // SAFETY: the signature matches -[NSView isFlipped].
    unsafe { builder.add_method(sel!(isFlipped), flipped as unsafe extern "C-unwind" fn(_, _) -> _) };
    build(&mut builder);
    Some(builder.register())
}

thread_local! {
    static ENTERING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

const BEFORE: &CStr = c"RuneNativeViewTabBefore";
const AFTER: &CStr = c"RuneNativeViewTabAfter";

fn is_stop(view: &AnyObject) -> bool {
    let name = view.class().name();
    name == BEFORE || name == AFTER
}

fn subviews(view: &AnyObject) -> Vec<Retained<AnyObject>> {
    // SAFETY: NSView reads on the main thread; each subview is retained for the caller.
    unsafe {
        let subviews: *mut AnyObject = msg_send![view, subviews];
        let Some(subviews) = subviews.as_ref() else { return Vec::new() };
        let count: usize = msg_send![subviews, count];
        (0..count).filter_map(|k| {
            let sub: *mut AnyObject = msg_send![subviews, objectAtIndex: k];
            Retained::retain(sub)
        }).collect()
    }
}

fn hosted(view: &AnyObject) -> Option<Retained<AnyObject>> {
    subviews(view).into_iter().find(|v| !is_stop(v))
}

unsafe fn fill_subviews(view: &AnyObject) {
    // SAFETY: NSView calls on the main thread, from AppKit's own callbacks into the view.
    unsafe {
        let bounds: CGRect = msg_send![view, bounds];
        for sub in subviews(view).iter().filter(|v| !is_stop(v)) { let _: () = msg_send![&**sub, setFrame: bounds]; }
    }
}

fn stop_class(name: &'static CStr) -> Option<&'static AnyClass> {
    unsafe extern "C-unwind" fn yes(_: &AnyObject, _: Sel) -> Bool { Bool::YES }
    unsafe extern "C-unwind" fn no(_: &AnyObject, _: Sel) -> Bool { Bool::NO }
    unsafe extern "C-unwind" fn miss(_: &AnyObject, _: Sel, _: CGPoint) -> *mut AnyObject { std::ptr::null_mut() }
    unsafe extern "C-unwind" fn become_first(this: &AnyObject, _: Sel) -> Bool {
        if ENTERING.get() { return Bool::YES; }
        let back = this.class().name() == BEFORE;
        super::tabbed_out_where(back, |slot| slot.view.contains(this));
        Bool::YES
    }
    class(name, |b| {
        // SAFETY: the signatures match -[NSResponder acceptsFirstResponder], -[NSView
        // canBecomeKeyView], -[NSResponder becomeFirstResponder], -[NSView hitTest:] and
        // -[NSAccessibility isAccessibilityElement].
        unsafe {
            b.add_method(sel!(acceptsFirstResponder), yes as unsafe extern "C-unwind" fn(_, _) -> _);
            b.add_method(sel!(canBecomeKeyView), yes as unsafe extern "C-unwind" fn(_, _) -> _);
            b.add_method(sel!(becomeFirstResponder), become_first as unsafe extern "C-unwind" fn(_, _) -> _);
            b.add_method(sel!(hitTest:), miss as unsafe extern "C-unwind" fn(_, _, _) -> _);
            b.add_method(sel!(isAccessibilityElement), no as unsafe extern "C-unwind" fn(_, _) -> _);
        }
    })
}

unsafe fn link_key_views(container: &AnyObject) {
    let Some(hosted) = hosted(container) else { return };
    let stops = subviews(container);
    let find = |name: &CStr| stops.iter().find(|v| v.class().name() == name).cloned();
    let (Some(before), Some(after)) = (find(BEFORE), find(AFTER)) else { return };
    // SAFETY: NSView key view links on the main thread, between views inside `container`.
    unsafe {
        let _: () = msg_send![&*before, setNextKeyView: &*hosted];
        let mut last = hosted;
        for _ in 0..256 {
            let next: *mut AnyObject = msg_send![&*last, nextKeyView];
            let Some(next) = next.as_ref() else { break };
            let inside: bool = msg_send![next, isDescendantOf: container];
            if !inside || is_stop(next) || std::ptr::eq(next, Retained::as_ptr(&last)) { break; }
            last = objc2::Message::retain(next);
        }
        let _: () = msg_send![&*last, setNextKeyView: &*after];
    }
}

fn container_class() -> Option<&'static AnyClass> {
    unsafe extern "C-unwind" fn resize(this: &AnyObject, _: Sel, _: CGSize) {
        // SAFETY: AppKit calls this on the main thread with a live view.
        unsafe { fill_subviews(this) }
    }
    unsafe extern "C-unwind" fn added(this: &AnyObject, _: Sel, _: *mut AnyObject) {
        // SAFETY: as above.
        unsafe { fill_subviews(this); link_key_views(this) }
    }
    unsafe extern "C-unwind" fn run(_: &AnyObject, _: Sel, _: *mut AnyObject) { super::run_requests() }
    class(c"RuneNativeViewContainer", |b| {
        // SAFETY: the signatures match -[NSView resizeSubviewsWithOldSize:], -[NSView
        // didAddSubview:] and a method taking one object, the perform's nil argument, that returns nothing.
        unsafe {
            b.add_method(sel!(runeNativeViewRun:), run as unsafe extern "C-unwind" fn(_, _, _));
            b.add_method(sel!(resizeSubviewsWithOldSize:), resize as unsafe extern "C-unwind" fn(_, _, _));
            b.add_method(sel!(didAddSubview:), added as unsafe extern "C-unwind" fn(_, _, _));
        }
    })
}

fn root_class() -> Option<&'static AnyClass> {
    unsafe extern "C-unwind" fn hit_test(this: &AnyObject, _: Sel, point: CGPoint) -> *mut AnyObject {
        let Some(view_class) = AnyClass::get(c"NSView") else { return std::ptr::null_mut() };
        // SAFETY: NSView's own hit test on the main thread, then reads of the current event.
        unsafe {
            let hit: *mut AnyObject = msg_send![super(this, view_class), hitTest: point];
            if hit.is_null() || std::ptr::eq(hit, this) { return std::ptr::null_mut(); }
            if mouse_down() && let Some(hit) = hit.as_ref() {
                super::press_where(|slot| slot.view.contains(hit));
            }
            hit
        }
    }
    class(c"RuneNativeViewRoot", |b| {
        // SAFETY: the signature matches -[NSView hitTest:].
        unsafe { b.add_method(sel!(hitTest:), hit_test as unsafe extern "C-unwind" fn(_, _, _) -> _) };
    })
}

fn mouse_down() -> bool {
    let Some(app) = AnyClass::get(c"NSApplication") else { return false };
    // SAFETY: NSApplication reads on the main thread.
    unsafe {
        let app: *mut AnyObject = msg_send![app, sharedApplication];
        let Some(app) = app.as_ref() else { return false };
        let event: *mut AnyObject = msg_send![app, currentEvent];
        let Some(event) = event.as_ref() else { return false };
        let kind: usize = msg_send![event, type];
        matches!(kind, 1 | 3 | 25)
    }
}

fn make_view(class: Option<&'static AnyClass>, frame: CGRect) -> Option<Retained<AnyObject>> {
    // SAFETY: an NSView subclass initializer on the main thread; the caller owns the result.
    unsafe {
        let alloc: *mut AnyObject = msg_send![class?, alloc];
        let view: *mut AnyObject = msg_send![alloc, initWithFrame: frame];
        let view = Retained::from_raw(view)?;
        let _: () = msg_send![&*view, setWantsLayer: true];
        Some(view)
    }
}

pub(crate) struct Container {
    view: Option<Retained<AnyObject>>,
}

impl Container {
    pub(crate) fn new() -> Self {
        let view = make_view(container_class(), rect([0.0; 4]));
        if let Some(view) = &view {
            // SAFETY: NSView setup on the main thread; the container retains its subviews.
            unsafe {
                let _: () = msg_send![&**view, setAutoresizesSubviews: true];
                for name in [BEFORE, AFTER] {
                    if let Some(stop) = make_view(stop_class(name), rect([0.0; 4])) { let _: () = msg_send![&**view, addSubview: &*stop]; }
                }
            }
        }
        Container { view }
    }

    pub(crate) fn object(&self) -> Option<&AnyObject> { self.view.as_deref() }

    pub(crate) fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let view = self.view.as_deref().ok_or(HandleError::Unavailable)?;
        let ptr = NonNull::from(view).cast::<c_void>();
        // SAFETY: the container lives as long as `self`, which the handle borrows.
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::AppKit(AppKitWindowHandle::new(ptr))) })
    }

    pub(crate) fn contains(&self, view: &AnyObject) -> bool {
        let Some(container) = self.view.as_deref() else { return false };
        // SAFETY: an NSView query on the main thread.
        unsafe { msg_send![view, isDescendantOf: container] }
    }

    pub(crate) fn in_window(&self) -> bool {
        let Some(view) = self.view.as_deref() else { return false };
        // SAFETY: an NSView query on the main thread.
        let window: *mut AnyObject = unsafe { msg_send![view, window] };
        !window.is_null()
    }

    fn window(&self) -> Option<Retained<AnyObject>> {
        let view = self.view.as_deref()?;
        // SAFETY: an NSView read on the main thread; the window is retained for the caller.
        unsafe {
            let window: *mut AnyObject = msg_send![view, window];
            Retained::retain(window)
        }
    }

    pub(crate) fn focus(&self) -> bool {
        let (Some(view), Some(window)) = (self.view.as_deref(), self.window()) else { return false };
        let target = hosted(view);
        // SAFETY: an NSWindow call on the main thread.
        unsafe { msg_send![&*window, makeFirstResponder: target.as_deref().unwrap_or(view)] }
    }

    pub(crate) fn can_enter(&self) -> bool { self.in_window() && self.view.as_deref().and_then(hosted).is_some() }

    pub(crate) fn run_soon(&self) {
        let Some(view) = self.view.as_deref() else { return };
        // SAFETY: NSObject's main thread perform, called on the main thread: it queues the
        // message for the run loop's next pass in its common modes, after the current frame or
        // event returns, and retains the view until then.
        unsafe {
            let _: () = msg_send![view, performSelectorOnMainThread: sel!(runeNativeViewRun:), withObject: std::ptr::null_mut::<AnyObject>(), waitUntilDone: Bool::NO];
        }
    }

    pub(crate) fn enter(&self, back: bool) -> bool {
        let (Some(view), Some(window)) = (self.view.as_deref(), self.window()) else { return false };
        // SAFETY: key view links and NSWindow key view selection on the main thread. Selecting
        // through the loop sets the window's key view selection direction, which a WKWebView
        // reads to focus its first or last element.
        unsafe {
            link_key_views(view);
            let stops = subviews(view);
            let stop = stops.iter().find(|v| v.class().name() == if back { AFTER } else { BEFORE });
            let Some(stop) = stop else { return false };
            ENTERING.set(true);
            if back {
                let _: () = msg_send![&*window, selectKeyViewPrecedingView: &**stop];
            } else {
                let _: () = msg_send![&*window, selectKeyViewFollowingView: &**stop];
            }
            ENTERING.set(false);
            let responder: *mut AnyObject = msg_send![&*window, firstResponder];
            self.has_focus() && responder.as_ref().is_some_and(|r| !is_stop(r))
        }
    }

    pub(crate) fn reclaim(&self) {
        if !self.has_focus() { return; }
        let (Some(view), Some(window)) = (self.view.as_deref(), self.window()) else { return };
        // SAFETY: NSView reads and an NSWindow call on the main thread. The container sits in a
        // clip view inside the root, whose superview is the window's Rune view.
        unsafe {
            let mut at: *mut AnyObject = msg_send![view, superview];
            while let Some(v) = at.as_ref() {
                if v.class().name() == c"RuneNativeViewRoot" {
                    let rune: *mut AnyObject = msg_send![v, superview];
                    if let Some(rune) = rune.as_ref() { let _: bool = msg_send![&*window, makeFirstResponder: rune]; }
                    return;
                }
                at = msg_send![v, superview];
            }
        }
    }

    pub(crate) fn has_focus(&self) -> bool {
        let (Some(view), Some(ns_view)) = (self.view.as_deref(), AnyClass::get(c"NSView")) else { return false };
        // SAFETY: NSWindow and NSView queries on the main thread.
        unsafe {
            let window: *mut AnyObject = msg_send![view, window];
            let Some(window) = window.as_ref() else { return false };
            let responder: *mut AnyObject = msg_send![window, firstResponder];
            let Some(responder) = responder.as_ref() else { return false };
            let is_view: bool = msg_send![responder, isKindOfClass: ns_view];
            is_view && msg_send![responder, isDescendantOf: view]
        }
    }
}

impl Drop for Container {
    fn drop(&mut self) {
        if let Some(view) = &self.view {
            // SAFETY: removes the container from whatever clip view holds it, on the main thread.
            unsafe { let _: () = msg_send![&**view, removeFromSuperview]; }
        }
    }
}

pub(crate) struct Clip {
    view: Retained<AnyObject>,
}

impl ClipView for Clip {
    type Root = AnyObject;
    type Container = AnyObject;

    fn make(root: &AnyObject) -> Option<Self> {
        let view = make_view(class(c"RuneNativeViewClip", |_| {}), rect([0.0; 4]))?;
        // SAFETY: CALayer and NSView setup on the main thread; the clip view is retained here.
        unsafe {
            let layer: *mut AnyObject = msg_send![&*view, layer];
            if let Some(layer) = layer.as_ref() { let _: () = msg_send![layer, setMasksToBounds: true]; }
            let _: () = msg_send![root, addSubview: &*view];
        }
        Some(Clip { view })
    }

    fn holds(&self, container: &AnyObject) -> bool {
        // SAFETY: an NSView read on the main thread.
        let parent: *mut AnyObject = unsafe { msg_send![container, superview] };
        std::ptr::eq(parent, Retained::as_ptr(&self.view))
    }

    fn adopt(&self, container: &AnyObject) {
        // SAFETY: moves the container into this clip view, on the main thread.
        unsafe { let _: () = msg_send![&*self.view, addSubview: container]; }
    }

    fn place(&self, container: &AnyObject, at: &Placement) {
        // SAFETY: NSView and CALayer setters on the main thread, inside the open transaction.
        unsafe {
            let _: () = msg_send![&*self.view, setFrame: rect(at.clip)];
            let _: () = msg_send![container, setFrame: rect(at.inner)];
            let layer: *mut AnyObject = msg_send![container, layer];
            if let Some(layer) = layer.as_ref() {
                let _: () = msg_send![layer, setCornerRadius: at.radius];
                let _: () = msg_send![layer, setMasksToBounds: at.radius > 0.0];
            }
        }
    }

    fn show(&self, alpha: f64, was_hidden: bool) {
        // SAFETY: NSView setters on the main thread.
        unsafe {
            if was_hidden { let _: () = msg_send![&*self.view, setHidden: false]; }
            let _: () = msg_send![&*self.view, setAlphaValue: alpha];
        }
    }

    fn hide(&self) {
        // SAFETY: an NSView setter on the main thread.
        unsafe { let _: () = msg_send![&*self.view, setHidden: true]; }
    }

    fn raise(&self, root: &AnyObject) {
        // SAFETY: re-adds a subview of `root` on top, which moves it in z without leaving.
        unsafe { let _: () = msg_send![root, addSubview: &*self.view, positioned: NS_WINDOW_ABOVE, relativeTo: std::ptr::null::<AnyObject>()]; }
    }

    fn remove(&self) {
        // SAFETY: removes a clip view this window added, on the main thread.
        unsafe { let _: () = msg_send![&*self.view, removeFromSuperview]; }
    }
}

#[derive(Default)]
pub(crate) struct Hosts {
    content: Option<Retained<AnyObject>>,
    root: Option<Retained<AnyObject>>,
    clips: Clips<Clip>,
}

impl Hosts {
    pub(crate) fn new() -> Self { Self::default() }

    pub(crate) fn attach(&mut self, window: &winit::window::Window) {
        if self.content.is_some() { return; }
        let Some(view) = crate::platform::macos::content_view(window) else { return };
        // SAFETY: winit's content view outlives this retain, which only adds a reference.
        self.content = unsafe { Retained::retain(Retained::as_ptr(&view).cast::<AnyObject>().cast_mut()) };
    }

    pub(crate) fn present(&mut self, draws: &[NativeViewDraw], scale: f32, present: impl FnOnce()) {
        self.clips.prune(|id| super::slot(id).is_some());
        if draws.is_empty() && self.clips.idle() { return present(); }
        let (Some(transaction), Some(root)) = (AnyClass::get(c"CATransaction"), self.root()) else { return present() };
        // SAFETY: CATransaction class methods on the main thread, balanced below.
        unsafe { let _: () = msg_send![transaction, begin]; let _: () = msg_send![transaction, setDisableActions: true]; }
        let slots: Vec<_> = draws.iter().filter_map(|d| super::slot(d.view)).collect();
        let container = |id: NativeViewId| slots.iter().find(|s| s.id == id).and_then(|s| s.view.object());
        let adopted = |id: NativeViewId| if let Some(s) = slots.iter().find(|s| s.id == id) { s.wake(); };
        self.clips.sync(&root, draws, scale, container, adopted);
        present();
        // SAFETY: closes the transaction opened above, with the drawable inside it.
        unsafe { let _: () = msg_send![transaction, commit]; }
    }

    fn root(&mut self) -> Option<Retained<AnyObject>> {
        if let Some(root) = &self.root { return Some(root.clone()); }
        let content = self.content.clone()?;
        // SAFETY: AppKit calls on the main thread; the root is retained here and added to winit's
        // content view above its other subviews, its layer above the Metal layer.
        unsafe {
            let bounds: CGRect = msg_send![&*content, bounds];
            let root = make_view(root_class(), bounds)?;
            let _: () = msg_send![&*root, setAutoresizingMask: WIDTH_HEIGHT_SIZABLE];
            let _: () = msg_send![&*content, addSubview: &*root, positioned: NS_WINDOW_ABOVE, relativeTo: std::ptr::null::<AnyObject>()];
            let layer: *mut AnyObject = msg_send![&*root, layer];
            if let Some(layer) = layer.as_ref() { let _: () = msg_send![layer, setZPosition: 2.0f64]; }
            self.root = Some(root.clone());
            Some(root)
        }
    }
}

impl Drop for Hosts {
    fn drop(&mut self) {
        for clip in self.clips.views() {
            // SAFETY: detaches each hosted container from this window's clip views, on the main
            // thread, so a slot the app keeps can be shown in another window.
            unsafe {
                let subviews: *mut AnyObject = msg_send![&*clip.view, subviews];
                if let Some(subviews) = subviews.as_ref() {
                    let copy: Retained<AnyObject> = msg_send![subviews, copy];
                    let _: () = msg_send![&*copy, makeObjectsPerformSelector: sel!(removeFromSuperview)];
                }
            }
        }
        if let Some(root) = &self.root {
            // SAFETY: removes the root this struct added; drop runs on the main thread with the window.
            unsafe { let _: () = msg_send![&**root, removeFromSuperview]; }
        }
    }
}
