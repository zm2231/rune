#![allow(unsafe_code)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use super::appearance::Appearance;
use super::dropped::{Kind, Reported};
use super::feedback::Feedback;
use super::field_touch;
use super::lifecycle;
use super::park;
use super::surface::{AppRole, Hosted, Mount, OverlayRole, OverlaySlot, Role, report};
use crate::a11y::AccessibilityLink;
use crate::app::App;
use crate::embed::LayerHost;
use crate::overlay::ios::{Built, Parked};

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
    let mount = mount(app, Some(&key));
    Entry { key, starts_visible, mount, requests, handle }
}

pub(crate) fn mount<S: 'static>(mut app: App<S>, key: Option<&str>) -> Mount {
    let on_ready = app.on_ready.take();
    let (on_encoded, on_presented, reported) = (app.on_frame_encoded.take(), app.on_frame_presented.take(), key.unwrap_or("main").to_owned());
    let open_urls = crate::open_url::incoming::listen(&app);
    let mut dropped = Reported::default();
    if !app.windows.is_empty() { report(&mut dropped, Kind::Windows); }
    let pending = std::mem::take(&mut app.ios_overlays);
    let (on_quit, on_rest, on_close, appearance) = (app.on_quit, app.on_rest, app.on_close, Appearance::new(app.color_scheme));
    let window = key.map(str::to_owned);
    let (spec, state, shared, config, mut member) = match key {
        Some(key) => app.into_member_parts(key, None),
        None => {
            let (spec, state, shared, config) = app.into_parts(None);
            (spec, state, shared, config, None)
        }
    };
    Box::new(move |layer, size, scale, dark, wake, presents| {
        // SAFETY: `layer` is the `CAMetalLayer` of the view that owns this surface, and the
        // view calls this on the main thread.
        let host = unsafe { LayerHost::from_core_animation_layer(layer, &spec, state, size, scale, dark, wake.clone())? };
        let mut host = host.with_system_clipboard(wake.clone()).with_popup_menus().with_file_dialogs().with_share_sheets().with_shell().with_system_links();
        host.runtime_mut().configure(config);
        host.runtime_mut().core_mut().answer_links_later(wake.clone());
        host.runtime_mut().core_mut().set_backdrop_materials(true);
        if let Some(key) = &window { host.runtime_mut().core_mut().set_window_key(key); }
        if let Some(member) = &mut member {
            member.connect(wake.clone());
            member.enter(host.runtime_mut());
        }
        host.runtime_mut().set_live_visible(false);
        if let Some(member) = &mut member { member.leave(host.runtime_mut()); }
        shared.connect(wake.clone());
        let menus = crate::native_menu::MenuGuard::attach(&shared, false);
        let overlays = pending.into_iter().map(|overlay| overlay.into_slot()).collect();
        let role = Role::App(Box::new(AppRole { shared, menus, on_ready, dropped, on_quit, on_rest, on_close, _open_urls: open_urls, window: None, undrawn: true, overlays, advertised: Default::default() }));
        let feedback = Feedback::new(on_encoded, on_presented, &reported, layer, presents);
        Ok(Box::new(Hosted { host, role, accessibility: AccessibilityLink::with_wake(wake.clone()), wake, title: spec.title, lifecycle: lifecycle::State::default(), field: field_touch::State::default(), appearance, park: park::State::default(), feedback, member }))
    })
}

pub(crate) fn overlay_slot<T: 'static>(built: Built<T>) -> OverlaySlot {
    let Built { name, rt: parked, compositor, setup, on_availability, feed, control, standby, .. } = built;
    if let Some(Parked { rt, clipboard }) = parked.borrow_mut().as_mut() {
        let wake = control.waker();
        clipboard.connect(wake.clone());
        if let Some(setup) = setup { setup(&mut rt.state, wake); }
        feed.available(rt, on_availability);
    }
    let mount: Mount = Box::new(move |layer, size, scale, dark, wake, presents| {
        let Parked { rt, clipboard } = parked.borrow_mut().take().ok_or_else(|| format!("overlay {name} mounted twice"))?;
        // SAFETY: as in `mount`: the overlay's own view owns `layer` and calls this on the
        // main thread.
        let mut host = match unsafe { LayerHost::transparent(layer, name, rt, compositor, size, scale, dark, wake.clone()) } {
            Ok(host) => host,
            Err(error) => {
                feed.close_after_hidden();
                return Err(error);
            }
        };
        host = host.with_clipboard(clipboard, wake.clone()).with_system_links();
        host.runtime_mut().set_live_visible(false);
        let role = Role::Overlay(Box::new(OverlayRole { feed }));
        let feedback = Feedback::new(None, None, name, layer, presents);
        Ok(Box::new(Hosted { host, role, accessibility: AccessibilityLink::with_wake(wake.clone()), wake, title: name, lifecycle: lifecycle::State::default(), field: field_touch::State::default(), appearance: Appearance::new(crate::ColorScheme::System), park: park::State::default(), feedback, member: None }))
    });
    OverlaySlot { control: Rc::new(RefCell::new(control)), mount, standby }
}
