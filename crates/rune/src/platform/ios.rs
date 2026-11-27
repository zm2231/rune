//! iOS: an app draws in a `RuneView`, a `UIView` on a `CAMetalLayer` that a display link drives
//! while something moves. [`App::run`](crate::App::run) makes the app's window and fills it
//! with one; [`view`] hands one to a UIKit or SwiftUI app that owns its own screens.

#[cfg(target_os = "ios")]
mod accessibility;
#[cfg(target_os = "ios")]
mod app;
pub(crate) mod appearance;
#[cfg(target_os = "ios")]
mod controller;
pub(crate) mod dropped;
pub(crate) mod edit_menu;
pub(crate) mod field_touch;
pub(crate) mod keys;
pub(crate) mod lifecycle;
pub(crate) mod links;
#[cfg(target_os = "ios")]
mod mount;
pub(crate) mod overlay;
pub(crate) mod pacing;
pub(crate) mod park;
#[cfg(target_os = "ios")]
mod scenes;
pub(crate) mod size_class;
#[cfg(target_os = "ios")]
mod surface;
pub(crate) mod text;
pub(crate) mod tint;
#[cfg(target_os = "ios")]
mod view;
pub(crate) mod windows;
pub(crate) mod zoom;

#[cfg(target_os = "ios")]
pub(crate) use app::run;
#[cfg(target_os = "ios")]
pub(crate) use app::run_windows;
#[cfg(target_os = "ios")]
pub(crate) use mount::{Entry, entry, overlay_slot};
#[cfg(target_os = "ios")]
pub(crate) use surface::OverlaySlot;

/// A new `UIView *` that draws `app`, for a host that owns its own window: add it to a view
/// hierarchy, or return it from a `UIViewRepresentable`. It fills its superview, starts the app
/// when it first has a size, draws while something moves and stops in the background.
///
/// The pointer is retained: the caller releases it (`Unmanaged.fromOpaque(_:).takeRetainedValue()`
/// in Swift). Null off the main thread.
#[cfg(target_os = "ios")]
pub fn view<S: 'static>(app: crate::App<S>) -> *mut std::ffi::c_void {
    let Some(mtm) = objc2::MainThreadMarker::new() else { return std::ptr::null_mut() };
    let mount = mount::mount(app, None);
    // No event loop of Rune's says when the host has made its views, so each one settles the
    // links: one handed over before the first view was made has reached its hook by now, and
    // from here a link goes to a view that listens or is dropped, never kept.
    crate::open_url::incoming::settle();
    objc2::rc::Retained::into_raw(view::RuneView::new(mtm, mount)).cast()
}

/// Hands `url` to [`App::on_open_url`](crate::App::on_open_url) of the app in a [`view`], for a
/// host with its own scene delegate: iOS hands the links it opens with the app to that
/// delegate, not to a view inside it. Call it from the host's `scene(_:openURLContexts:)` and
/// `scene(_:continue:)`, or from SwiftUI's `onOpenURL`, through a function the app's library
/// exports.
///
/// The hook runs on the main thread as an `AppHandle::update`, in the order the links were
/// handed over, and wakes the view, exactly as for an app [`run`](crate::run) started. The
/// first view made whose app has the hook, and that is still alive, gets the link; with no
/// such view it is dropped. A link handed over before any view was made waits for the first
/// view made, and is dropped if that view's app has no hook. An empty string is dropped. Any
/// thread may call this.
#[cfg(target_os = "ios")]
pub fn open_url(url: &str) {
    for link in links::incoming(vec![url.to_owned()], Vec::new()) { crate::open_url::incoming::receive(link); }
}
