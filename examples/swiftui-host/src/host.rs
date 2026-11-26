#![allow(unsafe_code)]

use std::cell::{Cell, RefCell};
use std::ffi::c_void;

use rune::{App, AppHandle, Tx};

use super::State;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwiftUIHostListener {
    context: *mut c_void,
    cards_changed: Option<unsafe extern "C" fn(*mut c_void, u32)>,
    card_opened: Option<unsafe extern "C" fn(*mut c_void, u32)>,
}

thread_local! {
    static HANDLE: RefCell<Option<AppHandle<State>>> = const { RefCell::new(None) };
    static LISTENER: Cell<Option<SwiftUIHostListener>> = const { Cell::new(None) };
}

fn listener() -> Option<SwiftUIHostListener> {
    LISTENER.with(Cell::get)
}

fn changed(state: &State) {
    let count = u32::try_from(state.cards.len()).unwrap_or(u32::MAX);
    if let Some(SwiftUIHostListener { context, cards_changed: Some(f), .. }) = listener() {
        // SAFETY: the host passed `f` and `context` together to `swiftui_host_listen` and keeps
        // both alive while they are set; this runs on the main thread, where it set them.
        unsafe { f(context, count) }
    }
}

pub(super) fn opened(number: u32) {
    if let Some(SwiftUIHostListener { context, card_opened: Some(f), .. }) = listener() {
        // SAFETY: as in `changed`.
        unsafe { f(context, number) }
    }
}

fn update(f: impl FnOnce(&mut State, &mut Tx) + Send + 'static) {
    HANDLE.with_borrow(|handle| if let Some(handle) = handle { handle.update(f) });
}

#[unsafe(no_mangle)]
pub extern "C" fn swiftui_host_view() -> *mut c_void {
    let app = App::new(super::spec());
    let handle = app.handle();
    handle.update(|s, _| changed(s));
    let view = rune::ios::view(app);
    if !view.is_null() {
        HANDLE.set(Some(handle));
    }
    view
}

#[unsafe(no_mangle)]
pub extern "C" fn swiftui_host_listen(listener: SwiftUIHostListener) {
    LISTENER.set(Some(listener));
    update(|s, _| changed(s));
}

#[unsafe(no_mangle)]
pub extern "C" fn swiftui_host_add_card() {
    update(|s, tx| {
        s.cards.push(s.cards.iter().max().map_or(1, |last| last + 1));
        tx.scroll_to_bottom("cards");
        changed(s);
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn swiftui_host_remove_card(number: u32) {
    update(move |s, _| {
        s.cards.retain(|&card| card != number);
        changed(s);
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn swiftui_host_clear() {
    update(|s, _| {
        s.cards.clear();
        changed(s);
    });
}
