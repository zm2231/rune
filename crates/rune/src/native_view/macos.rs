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

unsafe fn fill_subviews(view: &AnyObject) {
    // SAFETY: NSView calls on the main thread, from AppKit's own callbacks into the view.
    unsafe {
        let bounds: CGRect = msg_send![view, bounds];
        let subviews: *mut AnyObject = msg_send![view, subviews];
        let Some(subviews) = subviews.as_ref() else { return };
        let count: usize = msg_send![subviews, count];
        for k in 0..count {
            let sub: *mut AnyObject = msg_send![subviews, objectAtIndex: k];
            if let Some(sub) = sub.as_ref() { let _: () = msg_send![sub, setFrame: bounds]; }
        }
    }
}

fn container_class() -> Option<&'static AnyClass> {
    unsafe extern "C-unwind" fn resize(this: &AnyObject, _: Sel, _: CGSize) {
        // SAFETY: AppKit calls this on the main thread with a live view.
        unsafe { fill_subviews(this) }
    }
    unsafe extern "C-unwind" fn added(this: &AnyObject, _: Sel, _: *mut AnyObject) {
        // SAFETY: as above.
        unsafe { fill_subviews(this) }
    }
    class(c"RuneNativeViewContainer", |b| {
        // SAFETY: the signatures match -[NSView resizeSubviewsWithOldSize:] and -[NSView didAddSubview:].
        unsafe {
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
            // SAFETY: an NSView setter on the main thread.
            unsafe { let _: () = msg_send![&**view, setAutoresizesSubviews: true]; }
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

    pub(crate) fn focus(&self) -> bool {
        let Some(view) = self.view.as_deref() else { return false };
        // SAFETY: NSView and NSWindow calls on the main thread.
        unsafe {
            let window: *mut AnyObject = msg_send![view, window];
            let Some(window) = window.as_ref() else { return false };
            let subviews: *mut AnyObject = msg_send![view, subviews];
            let first: *mut AnyObject = match subviews.as_ref() { Some(s) => msg_send![s, firstObject], None => std::ptr::null_mut() };
            let target = first.as_ref().unwrap_or(view);
            msg_send![window, makeFirstResponder: target]
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
