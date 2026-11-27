#![allow(unsafe_code)]

use std::ffi::{CStr, c_void};
use std::ptr::NonNull;

use objc2::rc::Retained;
use crate::platform::objc_boundary::guarded;
use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
use objc2::{msg_send, sel};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use winit::raw_window_handle::{HandleError, RawWindowHandle, UiKitWindowHandle, WindowHandle};

use super::clips::{ClipView, Clips, Placement};
use super::{NativeViewDraw, NativeViewId};

fn rect(r: [f64; 4]) -> CGRect {
    CGRect { origin: CGPoint { x: r[0], y: r[1] }, size: CGSize { width: r[2], height: r[3] } }
}

fn class(name: &'static CStr, build: impl FnOnce(&mut ClassBuilder)) -> Option<&'static AnyClass> {
    if let Some(class) = AnyClass::get(name) { return Some(class); }
    let mut builder = ClassBuilder::new(name, AnyClass::get(c"UIView")?)?;
    build(&mut builder);
    Some(builder.register())
}

unsafe fn fill_subviews(view: &AnyObject) {
    // SAFETY: UIView calls on the main thread, from UIKit's own callbacks into the view.
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
    unsafe extern "C-unwind" fn layout(this: &AnyObject, _: Sel) {
        guarded("layoutSubviews", || {
            // SAFETY: UIKit calls this on the main thread with a live view.
            unsafe { fill_subviews(this) }
        })
    }
    unsafe extern "C-unwind" fn added(this: &AnyObject, _: Sel, _: *mut AnyObject) {
        guarded("didAddSubview:", || {
            // SAFETY: as above.
            unsafe { fill_subviews(this) }
        })
    }
    unsafe extern "C-unwind" fn run(_: &AnyObject, _: Sel, _: *mut AnyObject) { guarded("runeNativeViewRun:", super::run_requests) }
    unsafe extern "C-unwind" fn commands(_: &AnyObject, _: Sel) -> *mut AnyObject {
        guarded("keyCommands", || TAB_COMMANDS.with(|list| list.as_deref().map_or(std::ptr::null_mut(), |l| std::ptr::from_ref(l).cast_mut())))
    }
    unsafe extern "C-unwind" fn can(this: &AnyObject, _: Sel, action: Sel, sender: *mut AnyObject) -> Bool {
        guarded("canPerformAction:withSender:", || {
            if action == sel!(runeTab:) || action == sel!(runeBackTab:) {
                return Bool::new(super::takes_tab_where(|slot| is(slot, this)));
            }
            // SAFETY: UIResponder's own answer for every other action, with UIKit's arguments.
            unsafe { msg_send![super(this, AnyClass::get(c"UIView").expect("UIKit is loaded")), canPerformAction: action, withSender: sender] }
        })
    }
    unsafe extern "C-unwind" fn tab(this: &AnyObject, _: Sel, _: *mut AnyObject) {
        guarded("runeTab:", || super::tab_key_where(false, |slot| is(slot, this)))
    }
    unsafe extern "C-unwind" fn back_tab(this: &AnyObject, _: Sel, _: *mut AnyObject) {
        guarded("runeBackTab:", || super::tab_key_where(true, |slot| is(slot, this)))
    }
    class(c"RuneNativeViewContainer", |b| {
        // SAFETY: the signatures match -[UIView layoutSubviews], -[UIView didAddSubview:],
        // -[UIResponder keyCommands], -[UIResponder canPerformAction:withSender:], and methods
        // taking one object (the perform's nil argument, a key command) that return nothing.
        unsafe {
            b.add_method(sel!(keyCommands), commands as unsafe extern "C-unwind" fn(_, _) -> _);
            b.add_method(sel!(canPerformAction:withSender:), can as unsafe extern "C-unwind" fn(_, _, _, _) -> _);
            b.add_method(sel!(runeTab:), tab as unsafe extern "C-unwind" fn(_, _, _));
            b.add_method(sel!(runeBackTab:), back_tab as unsafe extern "C-unwind" fn(_, _, _));
            b.add_method(sel!(runeNativeViewRun:), run as unsafe extern "C-unwind" fn(_, _, _));
            b.add_method(sel!(layoutSubviews), layout as unsafe extern "C-unwind" fn(_, _));
            b.add_method(sel!(didAddSubview:), added as unsafe extern "C-unwind" fn(_, _, _));
        }
    })
}

fn is(slot: &super::Slot, view: &AnyObject) -> bool { slot.view.object().is_some_and(|v| std::ptr::eq(v, view)) }

thread_local! {
    static TAB_COMMANDS: Option<Retained<AnyObject>> = tab_commands();
}

fn tab_commands() -> Option<Retained<AnyObject>> {
    const SHIFT: isize = 1 << 17;
    let tab = objc2_foundation::NSString::from_str("\t");
    // SAFETY: UIKeyCommand's and NSArray's class constructors on the main thread, each returning
    // an autoreleased object that `Retained::retain` keeps.
    unsafe {
        let class = AnyClass::get(c"UIKeyCommand")?;
        let fwd: *mut AnyObject = msg_send![class, keyCommandWithInput: &*tab, modifierFlags: 0isize, action: sel!(runeTab:)];
        let back: *mut AnyObject = msg_send![class, keyCommandWithInput: &*tab, modifierFlags: SHIFT, action: sel!(runeBackTab:)];
        for command in [fwd, back] {
            if let Some(command) = command.as_ref() { let _: () = msg_send![command, setWantsPriorityOverSystemBehavior: Bool::YES]; }
        }
        let list: *mut AnyObject = msg_send![AnyClass::get(c"NSArray")?, arrayWithObjects: [fwd, back].as_ptr(), count: 2usize];
        Retained::retain(list)
    }
}

fn root_class() -> Option<&'static AnyClass> {
    unsafe extern "C-unwind" fn hit_test(this: &AnyObject, _: Sel, point: CGPoint, event: *mut AnyObject) -> *mut AnyObject {
        guarded("hitTest:withEvent:", || {
            let Some(view_class) = AnyClass::get(c"UIView") else { return std::ptr::null_mut() };
            // SAFETY: UIView's own hit test on the main thread, then a read of the event's type.
            unsafe {
                let hit: *mut AnyObject = msg_send![super(this, view_class), hitTest: point, withEvent: event];
                if hit.is_null() || std::ptr::eq(hit, this) { return std::ptr::null_mut(); }
                let touch = event.as_ref().is_some_and(|e| { let kind: isize = msg_send![e, type]; kind == 0 });
                if touch && let Some(hit) = hit.as_ref() {
                    super::focus_where(|slot| slot.view.contains(hit));
                }
                hit
            }
        })
    }
    class(c"RuneNativeViewRoot", |b| {
        // SAFETY: the signature matches -[UIView hitTest:withEvent:].
        unsafe { b.add_method(sel!(hitTest:withEvent:), hit_test as unsafe extern "C-unwind" fn(_, _, _, _) -> _) };
    })
}

pub(crate) fn hosted(view: &AnyObject) -> bool {
    // SAFETY: UIView reads on the main thread.
    unsafe {
        let mut at: *const AnyObject = view;
        while let Some(v) = at.as_ref() {
            if v.class().name() == c"RuneNativeViewRoot" { return true; }
            let up: *mut AnyObject = msg_send![v, superview];
            at = up;
        }
    }
    false
}

fn make_view(class: Option<&'static AnyClass>, frame: CGRect) -> Option<Retained<AnyObject>> {
    // SAFETY: a UIView subclass initializer on the main thread; the caller owns the result.
    unsafe {
        let alloc: *mut AnyObject = msg_send![class?, alloc];
        let view: *mut AnyObject = msg_send![alloc, initWithFrame: frame];
        Retained::from_raw(view)
    }
}

fn first_responder_in(view: &AnyObject) -> Option<Retained<AnyObject>> {
    // SAFETY: UIResponder and UIView reads on the main thread.
    unsafe {
        let first: bool = msg_send![view, isFirstResponder];
        if first { return Some(objc2::Message::retain(view)); }
        let subviews: *mut AnyObject = msg_send![view, subviews];
        let subviews = subviews.as_ref()?;
        let count: usize = msg_send![subviews, count];
        (0..count).find_map(|k| {
            let sub: *mut AnyObject = msg_send![subviews, objectAtIndex: k];
            sub.as_ref().and_then(first_responder_in)
        })
    }
}

pub(crate) struct Container {
    view: Option<Retained<AnyObject>>,
}

impl Container {
    pub(crate) fn new() -> Self { Container { view: make_view(container_class(), rect([0.0; 4])) } }

    pub(crate) fn object(&self) -> Option<&AnyObject> { self.view.as_deref() }

    pub(crate) fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let view = self.view.as_deref().ok_or(HandleError::Unavailable)?;
        let ptr = NonNull::from(view).cast::<c_void>();
        // SAFETY: the container lives as long as `self`, which the handle borrows.
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::UiKit(UiKitWindowHandle::new(ptr))) })
    }

    pub(crate) fn in_window(&self) -> bool {
        let Some(view) = self.view.as_deref() else { return false };
        // SAFETY: a UIView read on the main thread.
        let window: *mut AnyObject = unsafe { msg_send![view, window] };
        !window.is_null()
    }

    pub(crate) fn contains(&self, view: &AnyObject) -> bool {
        let Some(container) = self.view.as_deref() else { return false };
        // SAFETY: a UIView query on the main thread.
        unsafe { msg_send![view, isDescendantOfView: container] }
    }

    fn hosted(&self) -> Option<Retained<AnyObject>> {
        let view = self.view.as_deref()?;
        // SAFETY: UIView reads on the main thread; the subview is retained for the caller.
        unsafe {
            let subviews: *mut AnyObject = msg_send![view, subviews];
            let first: *mut AnyObject = msg_send![subviews.as_ref()?, firstObject];
            Retained::retain(first)
        }
    }

    pub(crate) fn focus(&self) -> bool {
        let Some(view) = self.view.as_deref() else { return false };
        let hosted = self.hosted();
        // SAFETY: a UIResponder call on the main thread.
        unsafe { msg_send![hosted.as_deref().unwrap_or(view), becomeFirstResponder] }
    }

    pub(crate) fn can_enter(&self) -> bool { self.in_window() && self.hosted().is_some() }

    pub(crate) fn run_soon(&self) {
        let Some(view) = self.view.as_deref() else { return };
        // SAFETY: NSObject's main thread perform, called on the main thread: it queues the
        // message for the run loop's next pass in its common modes, after the current frame or
        // event returns, and retains the view until then.
        unsafe {
            let _: () = msg_send![view, performSelectorOnMainThread: sel!(runeNativeViewRun:), withObject: std::ptr::null_mut::<AnyObject>(), waitUntilDone: Bool::NO];
        }
    }

    pub(crate) fn enter(&self, _back: bool) -> bool {
        let Some(hosted) = self.hosted() else { return false };
        // SAFETY: a UIResponder call on the main thread.
        let _: bool = unsafe { msg_send![&*hosted, becomeFirstResponder] };
        self.has_focus()
    }

    pub(crate) fn reclaim(&self) {
        if !self.has_focus() { return; }
        let Some(view) = self.view.as_deref() else { return };
        // SAFETY: UIView reads and a UIResponder call on the main thread. The container sits in
        // a clip view inside the root, whose superview is the Rune view.
        unsafe {
            let mut at: *mut AnyObject = msg_send![view, superview];
            while let Some(v) = at.as_ref() {
                if v.class().name() == c"RuneNativeViewRoot" {
                    let rune: *mut AnyObject = msg_send![v, superview];
                    if let Some(rune) = rune.as_ref() { let _: bool = msg_send![rune, becomeFirstResponder]; }
                    return;
                }
                at = msg_send![v, superview];
            }
        }
    }

    pub(crate) fn has_focus(&self) -> bool { self.view.as_deref().and_then(first_responder_in).is_some() }
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
        let view = make_view(AnyClass::get(c"UIView"), rect([0.0; 4]))?;
        // SAFETY: UIView setup on the main thread; the clip view is retained here.
        unsafe {
            let _: () = msg_send![&*view, setClipsToBounds: true];
            let _: () = msg_send![root, addSubview: &*view];
        }
        Some(Clip { view })
    }

    fn holds(&self, container: &AnyObject) -> bool {
        // SAFETY: a UIView read on the main thread.
        let parent: *mut AnyObject = unsafe { msg_send![container, superview] };
        std::ptr::eq(parent, Retained::as_ptr(&self.view))
    }

    fn adopt(&self, container: &AnyObject) {
        // SAFETY: moves the container into this clip view, on the main thread.
        unsafe { let _: () = msg_send![&*self.view, addSubview: container]; }
    }

    fn place(&self, container: &AnyObject, at: &Placement) {
        // SAFETY: UIView and CALayer setters on the main thread, inside the open transaction.
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
        // SAFETY: UIView setters on the main thread.
        unsafe {
            if was_hidden { let _: () = msg_send![&*self.view, setHidden: false]; }
            let _: () = msg_send![&*self.view, setAlpha: alpha];
        }
    }

    fn hide(&self) {
        // SAFETY: a UIView setter on the main thread.
        unsafe { let _: () = msg_send![&*self.view, setHidden: true]; }
    }

    fn raise(&self, root: &AnyObject) {
        // SAFETY: reorders a subview of `root`, on the main thread.
        unsafe { let _: () = msg_send![root, bringSubviewToFront: &*self.view]; }
    }

    fn remove(&self) {
        // SAFETY: removes a clip view this view added, on the main thread.
        unsafe { let _: () = msg_send![&*self.view, removeFromSuperview]; }
    }
}

pub(crate) struct Hosts {
    layer: Option<Retained<AnyObject>>,
    root: Option<Retained<AnyObject>>,
    clips: Clips<Clip>,
    synced: bool,
}

impl Hosts {
    pub(crate) unsafe fn new(layer: *mut c_void) -> Self {
        // SAFETY: the caller passes the live CAMetalLayer its surface draws on.
        let layer = unsafe { Retained::retain(layer.cast::<AnyObject>()) };
        Hosts { layer, root: None, clips: Clips::default(), synced: false }
    }

    pub(crate) fn detached() -> Self { Hosts { layer: None, root: None, clips: Clips::default(), synced: false } }

    pub(crate) fn present<R>(&mut self, draws: &[NativeViewDraw], scale: f32, present: impl FnOnce() -> R) -> R {
        self.clips.prune(|id| super::slot(id).is_some());
        if draws.is_empty() && self.clips.idle() { return present(); }
        let (Some(transaction), Some(root)) = (AnyClass::get(c"CATransaction"), self.root()) else { return present() };
        // SAFETY: CATransaction class methods on the main thread, balanced below.
        unsafe { let _: () = msg_send![transaction, begin]; let _: () = msg_send![transaction, setDisableActions: true]; }
        let slots: Vec<_> = draws.iter().filter_map(|d| super::slot(d.view)).collect();
        let container = |id: NativeViewId| slots.iter().find(|s| s.id == id).and_then(|s| s.view.object());
        let adopted = |id: NativeViewId| if let Some(s) = slots.iter().find(|s| s.id == id) { s.wake(); };
        self.clips.sync(&root, draws, scale, container, adopted);
        let out = present();
        // SAFETY: closes the transaction opened above, with the drawable inside it.
        unsafe { let _: () = msg_send![transaction, commit]; }
        out
    }

    pub(crate) fn sync_transactions(&mut self, on: bool) -> bool {
        if on == self.synced { return false; }
        let Some(layer) = self.layer.as_deref() else { return false };
        // SAFETY: a CAMetalLayer setter on the main thread, read when the next drawable is acquired.
        unsafe { let _: () = msg_send![layer, setPresentsWithTransaction: Bool::new(on)]; }
        self.synced = on;
        on
    }

    fn root(&mut self) -> Option<Retained<AnyObject>> {
        if let Some(root) = &self.root { return Some(root.clone()); }
        let layer = self.layer.as_deref()?;
        // SAFETY: UIKit calls on the main thread; the layer's delegate is the view it backs, and
        // the root is retained here and added on top of that view's subviews.
        unsafe {
            let view: *mut AnyObject = msg_send![layer, delegate];
            let view = view.as_ref()?;
            let bounds: CGRect = msg_send![view, bounds];
            let root = make_view(root_class(), bounds)?;
            let _: () = msg_send![&*root, setAutoresizingMask: 18usize];
            let _: () = msg_send![view, addSubview: &*root];
            self.root = Some(root.clone());
            Some(root)
        }
    }
}

impl Drop for Hosts {
    fn drop(&mut self) {
        for clip in self.clips.views() {
            // SAFETY: detaches each hosted container from this view's clip views, on the main thread.
            unsafe {
                let subviews: *mut AnyObject = msg_send![&*clip.view, subviews];
                if let Some(subviews) = subviews.as_ref() {
                    let copy: Retained<AnyObject> = msg_send![subviews, copy];
                    let _: () = msg_send![&*copy, makeObjectsPerformSelector: sel!(removeFromSuperview)];
                }
            }
        }
        if let Some(root) = &self.root {
            // SAFETY: removes the root this struct added, on the main thread.
            unsafe { let _: () = msg_send![&**root, removeFromSuperview]; }
        }
    }
}
