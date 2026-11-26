#[cfg(any(test, target_os = "macos", target_os = "ios"))]
mod clips;
#[cfg(test)]
mod clips_tests;
#[cfg(target_os = "ios")]
pub(crate) mod ios;
#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(test)]
mod tests;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use rune_motion::NodeId;
use winit::raw_window_handle::{HandleError, HasWindowHandle, WindowHandle};

use crate::El;

/// A platform view slot that an element hosts with [`El::native_view`]: an empty container
/// view an app builds its own view into, which Rune keeps at the element's rect.
///
/// Make it on the main thread, build the platform view into it through its window handle (it is
/// the parent view, as wry's `WebViewBuilder::build_as_child` takes), and keep both in the app's
/// state. Clones share one container. The container sizes every subview to its own bounds, so a
/// child built at any frame fills the element.
///
/// When the last clone drops, the container leaves its window and is released, with whatever
/// the app built into it that nothing else holds. An element that unmounts only hides it.
///
/// ```
/// use rune::{NativeView, el};
///
/// let slot = NativeView::new();
/// // macOS: let web = wry::WebViewBuilder::new().with_url("https://example.com").build_as_child(&slot)?;
/// let pane = el().w_full().h(320.0).radius(12.0).native_view(&slot);
/// # let _ = pane;
/// ```
#[derive(Clone)]
pub struct NativeView {
    slot: Rc<Slot>,
}

/// Which [`NativeView`] a [`NativeViewDraw`] places, stable for the view's life.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NativeViewId(u64);

pub(crate) struct Slot {
    id: NativeViewId,
    #[cfg(target_os = "macos")]
    pub(crate) view: macos::Container,
    #[cfg(target_os = "ios")]
    pub(crate) view: ios::Container,
    pressed: Cell<bool>,
    wake: RefCell<Option<Arc<crate::live::Wake>>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Tag(pub(crate) NativeViewId);

thread_local! {
    static SLOTS: RefCell<HashMap<NativeViewId, Weak<Slot>>> = RefCell::new(HashMap::default());
    static NEXT: Cell<u64> = const { Cell::new(1) };
}

impl NativeView {
    /// A new, empty slot. On macOS and iOS this makes the container view, so call it on the
    /// main thread; elsewhere it makes a token whose window handle is unavailable.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let id = NativeViewId(NEXT.with(|n| { let id = n.get(); n.set(id + 1); id }));
        let slot = Rc::new(Slot {
            id,
            #[cfg(target_os = "macos")]
            view: macos::Container::new(),
            #[cfg(target_os = "ios")]
            view: ios::Container::new(),
            pressed: Cell::new(false),
            wake: RefCell::new(None),
        });
        SLOTS.with(|s| {
            let mut s = s.borrow_mut();
            s.retain(|_, w| w.strong_count() > 0);
            s.insert(id, Rc::downgrade(&slot));
        });
        NativeView { slot }
    }

    /// This slot's id, as [`NativeViewDraw::view`] names it.
    pub fn id(&self) -> NativeViewId { self.slot.id }

    /// Whether the container is in a window: from the first frame that shows an element hosting
    /// it, which asks for one more frame so `poll` sees it. On macOS wry's `build_as_child`
    /// needs its parent in a window, so build there once this is true. False off macOS and iOS.
    pub fn in_window(&self) -> bool {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        { self.slot.view.in_window() }
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        { false }
    }

    /// Gives the keyboard to the view built into the slot (its first subview), as a click into
    /// it would. Rune's own focus clears in the next frame. Does nothing off macOS and iOS.
    pub fn focus(&self) {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        if self.slot.view.focus() { self.slot.press(); }
    }

    /// Whether the keyboard is in the view built into the slot: the window's first responder is
    /// the container or inside it. False off macOS and iOS.
    pub fn has_focus(&self) -> bool {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        { self.slot.view.has_focus() }
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        { false }
    }
}

impl std::fmt::Debug for NativeView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("NativeView").field(&self.slot.id.0).finish()
    }
}

impl HasWindowHandle for NativeView {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        { self.slot.view.window_handle() }
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        { Err(HandleError::NotSupported) }
    }
}

impl Slot {
    pub(crate) fn take_pressed(&self) -> bool { self.pressed.replace(false) }

    pub(crate) fn listen(&self, wake: &Arc<crate::live::Wake>) {
        let mut held = self.wake.borrow_mut();
        if !held.as_ref().is_some_and(|w| Arc::ptr_eq(w, wake)) { *held = Some(wake.clone()); }
    }

    fn press(&self) {
        self.pressed.set(true);
        self.wake();
    }

    pub(crate) fn wake(&self) {
        if let Some(wake) = self.wake.borrow().as_ref() { wake.notify(); }
    }
}

#[cfg_attr(not(any(test, target_os = "macos", target_os = "ios")), allow(dead_code))]
pub(crate) fn press_where(hit: impl Fn(&Slot) -> bool) {
    let slots: Vec<Rc<Slot>> = SLOTS.with(|s| s.borrow().values().filter_map(Weak::upgrade).collect());
    for slot in slots.iter().filter(|s| hit(s)) { slot.press(); }
}

const A11Y_CLASS: &str = "\x1frune-native-view:";

pub(crate) fn a11y_class(view: NativeViewId) -> String { format!("{A11Y_CLASS}{}", view.0) }

#[cfg_attr(not(any(test, target_os = "ios")), allow(dead_code))]
pub(crate) fn from_a11y_class(class: &str) -> Option<NativeViewId> {
    class.strip_prefix(A11Y_CLASS)?.parse().ok().map(NativeViewId)
}

#[cfg_attr(not(any(test, target_os = "macos", target_os = "ios")), allow(dead_code))]
pub(crate) fn slot(id: NativeViewId) -> Option<Rc<Slot>> {
    SLOTS.with(|s| s.borrow().get(&id).and_then(Weak::upgrade))
}

pub(crate) fn any_alive() -> bool {
    SLOTS.with(|s| s.borrow().values().any(|w| w.strong_count() > 0))
}

impl El {
    /// Hosts `view` in this element: the platform view built into the slot shows at the
    /// element's drawn rect, through scrolls, springs, collapses and window resizes, in the same
    /// frame as the pixels around it. The element lays out as any other; size it as the view
    /// should be. Its corner radius rounds the view.
    ///
    /// Clipping: the view is cut to the rect clips of its ancestors (scroll viewports,
    /// `El::clip_soft`, collapsing growth rows) and to the window. Rounded ancestor clips, masks and
    /// feathered edges are not followed: the view keeps square edges there.
    ///
    /// Z-order: hosted views sit above everything Rune draws in the window, in paint order among
    /// themselves. Rune can't draw over one: a popover, menu, tooltip or drag image Rune draws in
    /// the same window goes under it. What must cover it opens as an overlay window, or the app
    /// collapses or unmounts the host meanwhile.
    ///
    /// Hidden while the element is unmounted, culled, clipped away, zero-sized or faded out; its
    /// alpha follows the element's opacity. A press inside the view gives it the keyboard and
    /// clears Rune's focus; a press on Rune content takes the keyboard back. Tab doesn't yet move
    /// between Rune elements and the hosted view: it stays inside the view.
    ///
    /// In macOS windows and iOS views only; overlays don't host them, and elsewhere the element
    /// draws as an empty box, its rects still listed in
    /// [`Runtime::native_views`](crate::Runtime::native_views).
    ///
    /// ```
    /// use rune::{NativeView, ScrollSpec, col, el};
    ///
    /// let slot = NativeView::new();
    /// let pane = col().scroll(ScrollSpec::default()).h(400.0)
    ///     .child(el().h(80.0))
    ///     .child(el().w_full().h(320.0).radius(12.0).native_view(&slot));
    /// # let _ = pane;
    /// ```
    pub fn native_view(self, view: &NativeView) -> Self {
        self.data(Tag(view.slot.id))
    }
}

/// One hosted [`El::native_view`] element shown this frame, in paint order and the window's
/// physical pixels (divide by the window scale for points). Elements whose view is hidden
/// (unmounted, culled, clipped away, zero-sized, faded out) aren't listed. Read-only: Rune
/// builds these, apps don't.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct NativeViewDraw {
    /// The element's identity, stable across frames while it stays mounted.
    pub id: NodeId,
    /// The slot the element hosts.
    pub view: NativeViewId,
    /// x, y, width, height of the element from the window's top left: the view's frame.
    pub rect: [f32; 4],
    /// x, y, width, height of the part that shows: `rect` cut to the element's clipping
    /// ancestors and the window. Never empty.
    pub clip: [f32; 4],
    /// Corner radius (the largest corner), clamped to half the shorter side.
    pub radius: f32,
    /// The element's effective opacity, ancestors included.
    pub opacity: f32,
}

impl NativeViewDraw {
    /// `rect`, `clip` and `radius` in window points, given the window scale.
    pub fn points(&self, scale: f32) -> ([f64; 4], [f64; 4], f64) {
        let s = f64::from(scale.max(1e-3));
        let pt = |r: [f32; 4]| r.map(|v| f64::from(v) / s);
        (pt(self.rect), pt(self.clip), f64::from(self.radius) / s)
    }
}

impl<S: 'static> crate::Runtime<S> {
    /// The [`El::native_view`] elements the last frame showed, in paint order and physical
    /// pixels ([`NativeViewDraw`]).
    ///
    /// ```
    /// # use rune::{Env, NativeView, Runtime, el};
    /// thread_local! { static SLOT: NativeView = NativeView::new(); }
    /// let view = |_: &(), _: &mut rune::Cx<()>| SLOT.with(|s| el().size(100.0, 40.0).native_view(s));
    /// let mut rt = Runtime::new((), view, (100.0, 40.0), Env::default());
    /// rt.frame(0.0);
    /// let (rect, clip, _) = rt.native_views()[0].points(rt.window_scale());
    /// assert_eq!((rect, clip), ([0.0, 0.0, 100.0, 40.0], [0.0, 0.0, 100.0, 40.0]));
    /// ```
    pub fn native_views(&self) -> &[NativeViewDraw] { self.core().paint.native_views() }
}
