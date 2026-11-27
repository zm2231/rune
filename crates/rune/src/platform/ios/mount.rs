#![allow(unsafe_code)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use super::appearance::Appearance;
use super::dropped::{Kind, Reported};
use super::field_touch;
use super::lifecycle;
use super::park;
use super::surface::{AppRole, Hosted, Mount, OverlayRole, OverlaySlot, Role, report};
use crate::a11y::AccessibilityLink;
use crate::app::App;
use crate::embed::LayerHost;
use crate::overlay::ios::Built;

pub(crate) struct Entry {
    pub(crate) key: String,
    pub(crate) starts_visible: bool,
    pub(crate) mount: Mount,
    pub(crate) requests: Arc<crate::app::SharedCore>,
    pub(crate) handle: Option<crate::WindowHandle>,
}

pub(crate) fn entry<S: 'static>(spec: crate::WindowSpec<S>, handle: Option<crate::WindowHandle>) -> Entry {
    let crate::WindowSpec { key, app, options, .. } = spec;
    let starts_visible = options.visible && app.open;
    let requests = app.shared.core.clone();
    Entry { key, starts_visible, mount: mount(app), requests, handle }
}

pub(crate) fn mount<S: 'static>(mut app: App<S>) -> Mount {
    let on_ready = app.on_ready.take();
    let open_urls = crate::open_url::incoming::listen(&app);
    let mut dropped = Reported::default();
    if !app.windows.is_empty() { report(&mut dropped, Kind::Windows); }
    let pending = std::mem::take(&mut app.ios_overlays);
    let (on_quit, on_rest, on_close, appearance) = (app.on_quit, app.on_rest, app.on_close, Appearance::new(app.color_scheme));
    let (spec, state, shared, config) = app.into_parts(None);
    Box::new(move |layer, size, scale, dark, wake| {
        // SAFETY: `layer` is the `CAMetalLayer` of the view that owns this surface, and the
        // view calls this on the main thread.
        let host = unsafe { LayerHost::from_core_animation_layer(layer, &spec, state, size, scale, dark, wake.clone())? };
        let mut host = host.with_system_clipboard(wake.clone()).with_popup_menus().with_file_dialogs().with_share_sheets().with_shell().with_system_links();
        host.runtime_mut().configure(config);
        host.runtime_mut().core_mut().answer_links_later(wake.clone());
        host.runtime_mut().core_mut().set_backdrop_materials(true);
        host.runtime_mut().set_live_visible(false);
        shared.connect(wake.clone());
        let menus = crate::native_menu::MenuGuard::attach(&shared, false);
        let overlays = pending.into_iter().map(|overlay| overlay.into_slot()).collect();
        let role = Role::App(Box::new(AppRole { shared, menus, on_ready, dropped, on_quit, on_rest, on_close, _open_urls: open_urls, window: None, undrawn: true, overlays }));
        Ok(Box::new(Hosted { host, role, accessibility: AccessibilityLink::with_wake(wake.clone()), wake, title: spec.title, lifecycle: lifecycle::State::default(), field: field_touch::State::default(), appearance, park: park::State::default() }))
    })
}

pub(crate) fn overlay_slot<T: 'static>(built: Built<T>) -> OverlaySlot {
    let Built { name, rt, compositor, setup, on_availability, feed, control, after_hidden, .. } = built;
    let mount: Mount = Box::new(move |layer, size, scale, dark, wake| {
        let rt = rt.borrow_mut().take().ok_or_else(|| format!("overlay {name} mounted twice"))?;
        // SAFETY: as in `mount`: the overlay's own view owns `layer` and calls this on the
        // main thread.
        let mut host = unsafe { LayerHost::transparent(layer, name, rt, compositor, size, scale, dark, wake.clone())? };
        host = host.with_system_clipboard(wake.clone()).with_system_links();
        if let Some(setup) = setup { setup(&mut host.runtime_mut().state, wake.clone()); }
        host.runtime_mut().set_live_visible(false);
        feed.available(host.runtime_mut(), on_availability);
        let role = Role::Overlay(Box::new(OverlayRole { feed }));
        Ok(Box::new(Hosted { host, role, accessibility: AccessibilityLink::with_wake(wake.clone()), wake, title: name, lifecycle: lifecycle::State::default(), field: field_touch::State::default(), appearance: Appearance::new(crate::ColorScheme::System), park: park::State::default() }))
    });
    OverlaySlot { control: Rc::new(RefCell::new(control)), mount, after_hidden }
}
