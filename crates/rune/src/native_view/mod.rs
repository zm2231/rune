#[cfg(any(test, target_os = "macos", target_os = "ios"))]
mod clips;
#[cfg(test)]
mod clips_tests;
pub(crate) mod handoff;
#[cfg(test)]
mod handoff_tests;
#[cfg(not(any(target_os = "macos", target_os = "ios")))]
pub(crate) mod headless;
#[cfg(target_os = "ios")]
pub(crate) mod ios;
#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(all(test, not(any(target_os = "macos", target_os = "ios"))))]
mod tab_tests;
#[cfg(test)]
mod tests;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use rune_motion::NodeId;
use winit::raw_window_handle::{HandleError, HasWindowHandle, WindowHandle};

use crate::El;
#[cfg(not(any(target_os = "macos", target_os = "ios")))]
use headless::Container;
#[cfg(target_os = "ios")]
use ios::Container;
#[cfg(target_os = "macos")]
use macos::Container;

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
    pub(crate) view: Container,
    pressed: Cell<bool>,
    tabbed: Cell<Option<bool>>,
    refused: Cell<bool>,
    request: Cell<Option<Request>>,
    tab_key: RefCell<Option<TabKey>>,
    wake: RefCell<Option<Arc<crate::live::Wake>>>,
}

type TabKey = Rc<dyn Fn(bool)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Request {
    Focus,
    Enter { back: bool },
    Reclaim,
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
            view: Container::new(),
            pressed: Cell::new(false),
            tabbed: Cell::new(None),
            refused: Cell::new(false),
            request: Cell::new(None),
            tab_key: RefCell::new(None),
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
    pub fn in_window(&self) -> bool { self.slot.view.in_window() }

    /// Gives the keyboard to the view built into the slot (its first subview), as a click into
    /// it would, once the current frame or event has been handled. Rune's own focus then clears
    /// in the next frame. Does nothing off macOS and iOS.
    pub fn focus(&self) { self.slot.ask(Request::Focus); }

    /// Whether the keyboard is in the view built into the slot: the window's first responder is
    /// the container or inside it. False off macOS and iOS.
    pub fn has_focus(&self) -> bool { self.slot.view.has_focus() }

    /// Tells Rune that Tab (Shift-Tab when `back`) ran past the last (first) element inside the
    /// view built into the slot: in the next frame Rune's focus moves on to the element after
    /// (before) the hosting element in its Tab order, and the keyboard comes back to Rune.
    ///
    /// On macOS, AppKit views, `WKWebView` among them, report this themselves through the
    /// window's key view loop. On iOS the app reports it, from the [`NativeView::on_tab`]
    /// handler that moves Tab through the view, when Tab runs past either end.
    pub fn tabbed_out(&self, back: bool) { self.slot.tab_out(back); }

    /// On iOS, gives a hardware Tab (`false`) or Shift-Tab (`true`) pressed while the view built
    /// into the slot has the keyboard to `handle`, instead of to the view.
    ///
    /// UIKit doesn't pass Tab on to a hosted `WKWebView`'s page while one of its fields is
    /// edited, so the page can't move focus or leave by itself. With a handler, the app moves
    /// focus inside the view, for a web view with a script that focuses the page's next or
    /// previous element, and calls [`NativeView::tabbed_out`] past either end, as
    /// `examples/native-view` does. The page's own Tab handlers then don't see Tab. Without one,
    /// Tab stays the view's. Never called elsewhere: on macOS the key view loop moves Tab.
    pub fn on_tab(&self, handle: impl Fn(bool) + 'static) { *self.slot.tab_key.borrow_mut() = Some(Rc::new(handle)); }
}

impl std::fmt::Debug for NativeView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("NativeView").field(&self.slot.id.0).finish()
    }
}

impl HasWindowHandle for NativeView {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> { self.slot.view.window_handle() }
}

impl Slot {
    pub(crate) fn take_pressed(&self) -> bool { self.pressed.replace(false) }

    pub(crate) fn take_tabbed(&self) -> Option<bool> { self.tabbed.take() }

    pub(crate) fn take_refused(&self) -> bool { self.refused.replace(false) }

    pub(crate) fn ask(&self, request: Request) {
        if self.request.replace(Some(request)).is_none() { self.view.run_soon(); }
    }

    #[cfg_attr(not(any(test, target_os = "macos", target_os = "ios")), allow(dead_code))]
    fn run(&self) {
        match self.request.take() {
            Some(Request::Focus) => if self.view.focus() { self.press() },
            Some(Request::Enter { back }) if self.view.enter(back) => self.press(),
            Some(Request::Enter { .. }) => {
                self.refused.set(true);
                self.wake();
            }
            Some(Request::Reclaim) => self.view.reclaim(),
            None => {}
        }
    }

    fn tab_out(&self, back: bool) {
        self.tabbed.set(Some(back));
        self.wake();
    }

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

#[cfg_attr(not(any(test, target_os = "macos")), allow(dead_code))]
pub(crate) fn press_where(hit: impl Fn(&Slot) -> bool) {
    let slots: Vec<Rc<Slot>> = SLOTS.with(|s| s.borrow().values().filter_map(Weak::upgrade).collect());
    for slot in slots.iter().filter(|s| hit(s)) { slot.press(); }
}

#[cfg(target_os = "ios")]
pub(crate) fn takes_tab_where(hit: impl Fn(&Slot) -> bool) -> bool {
    let slots: Vec<Rc<Slot>> = SLOTS.with(|s| s.borrow().values().filter_map(Weak::upgrade).collect());
    slots.iter().any(|s| hit(s) && s.tab_key.borrow().is_some() && s.view.has_focus())
}

#[cfg(target_os = "ios")]
pub(crate) fn tab_key_where(back: bool, hit: impl Fn(&Slot) -> bool) {
    let slots: Vec<Rc<Slot>> = SLOTS.with(|s| s.borrow().values().filter_map(Weak::upgrade).collect());
    for slot in slots.iter().filter(|s| hit(s)) {
        let handle = slot.tab_key.borrow().clone();
        if let Some(handle) = handle { handle(back); }
    }
}

#[cfg(target_os = "ios")]
pub(crate) fn focus_where(hit: impl Fn(&Slot) -> bool) {
    let slots: Vec<Rc<Slot>> = SLOTS.with(|s| s.borrow().values().filter_map(Weak::upgrade).collect());
    for slot in slots.iter().filter(|s| hit(s)) { slot.ask(Request::Focus); }
}

#[cfg(target_os = "macos")]
pub(crate) fn tabbed_out_where(back: bool, hit: impl Fn(&Slot) -> bool) {
    let slots: Vec<Rc<Slot>> = SLOTS.with(|s| s.borrow().values().filter_map(Weak::upgrade).collect());
    for slot in slots.iter().filter(|s| hit(s)) { slot.tab_out(back); }
}

pub(crate) fn yield_to_rune() {
    let slots: Vec<Rc<Slot>> = SLOTS.with(|s| s.borrow().values().filter_map(Weak::upgrade).collect());
    for slot in &slots {
        let entering = matches!(slot.request.get(), Some(Request::Focus | Request::Enter { .. }));
        if slot.pressed.replace(false) | entering { slot.ask(Request::Reclaim); }
    }
}

#[cfg_attr(not(any(test, target_os = "macos", target_os = "ios")), allow(dead_code))]
pub(crate) fn run_requests() {
    let slots: Vec<Rc<Slot>> = SLOTS.with(|s| s.borrow().values().filter_map(Weak::upgrade).collect());
    for slot in &slots { slot.run(); }
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
    /// clears Rune's focus; a press on Rune content takes the keyboard back.
    ///
    /// Tab: the element is a stop in Rune's Tab order once the app has built a view into the
    /// slot. Tab onto it gives the view the keyboard and clears Rune's focus. When Tab runs past
    /// the view's last element (Shift-Tab past its first), focus moves on to the next Rune
    /// element (the previous one) and the keyboard comes back to Rune.
    ///
    /// On macOS this follows the window's key view loop: a `WKWebView` starts at its first
    /// element on Tab, or its last on Shift-Tab, and hands Tab back past its ends by itself.
    ///
    /// On iOS the view gets the keyboard with nothing inside it focused, and UIKit doesn't pass
    /// Tab on to a `WKWebView`'s page while one of its fields is edited. The app moves Tab
    /// through the view from [`NativeView::on_tab`] and reports leaving it with
    /// [`NativeView::tabbed_out`]. A tap inside the view also gives it the keyboard.
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
