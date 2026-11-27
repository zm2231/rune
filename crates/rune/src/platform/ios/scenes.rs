#![allow(unsafe_code)]

use std::cell::{Cell, RefCell};
use std::ptr::NonNull;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use block2::RcBlock;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, Message, class, define_class, extern_protocol, msg_send};
use objc2_foundation::NSString;

use super::controller::RuneViewController;
use super::mount::Entry;
use super::surface::Mount;
use super::view::RuneView;
use super::windows::{self, Ask, Effect, Event, Mode, Scene, State, Window, Windows};
use crate::platform::objc_boundary::guarded;
use crate::app::WindowRequest;
use crate::platform::main_queue::on_main;
use crate::runtime::Waker;

pub(crate) const ACTIVITY: &str = "rune.window";
const KEY: &str = "rune.window";
const PAGE_SHEET: isize = 1;

extern_protocol!(
    #[name = "UIAdaptivePresentationControllerDelegate"]
    unsafe trait PresentationDelegate: NSObjectProtocol {}
);

define_class!(
    // SAFETY: `NSObject` is made to be subclassed, and `RuneSheet` has no `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RuneSheet"]
    #[ivars = String]
    struct RuneSheet;

    // SAFETY: the superclass chain ends in `NSObject`, which implements the protocol.
    unsafe impl NSObjectProtocol for RuneSheet {}

    // SAFETY: every method of the protocol is optional.
    unsafe impl PresentationDelegate for RuneSheet {
        #[unsafe(method(presentationControllerDidDismiss:))]
        fn did_dismiss(&self, _controller: &AnyObject) {
            guarded("presentationControllerDidDismiss:", || { post(Posted::Dismissed(self.ivars().clone())); })
        }
    }
);

impl RuneSheet {
    fn new(mtm: MainThreadMarker, key: &str) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(key.to_owned());
        // SAFETY: `init` is `NSObject`'s designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

struct Slot {
    mount: Option<Mount>,
    view: Option<Retained<RuneView>>,
    controller: Option<Retained<RuneViewController>>,
    sheet: Option<Retained<RuneSheet>>,
    window: Option<Retained<AnyObject>>,
    requests: Arc<crate::app::SharedCore>,
    handle: Option<crate::WindowHandle>,
}

struct Registry {
    windows: Windows,
    slots: Vec<Slot>,
    scenes: Vec<(Scene, Retained<AnyObject>)>,
    refused: bool,
}

enum Posted {
    Connected { scene: Retained<AnyObject>, key: Option<String> },
    Disconnected(Retained<AnyObject>),
    Foreground(Retained<AnyObject>, Option<String>),
    Background(Retained<AnyObject>),
    Discarded(String),
    Asked(String, Ask),
    Dismissed(String),
    Drawn(String),
}

thread_local! {
    static PENDING: RefCell<Option<Vec<Entry>>> = const { RefCell::new(None) };
    static REGISTRY: RefCell<Option<Registry>> = const { RefCell::new(None) };
    static QUEUE: RefCell<VecDeque<Posted>> = const { RefCell::new(VecDeque::new()) };
    static NEXT_SCENE: Cell<Scene> = const { Cell::new(1) };
}

static SCHEDULED: AtomicBool = AtomicBool::new(false);

fn schedule() {
    if !SCHEDULED.swap(true, Ordering::AcqRel) {
        on_main(|| {
            SCHEDULED.store(false, Ordering::Release);
            pump();
        });
    }
}

fn pump_wake() -> Waker { Arc::new(schedule) }

fn post(posted: Posted) {
    QUEUE.with_borrow_mut(|queue| queue.push_back(posted));
    schedule();
}

pub(crate) fn asked(key: &str, request: Option<WindowRequest>, focus: bool) {
    if let Some(ask) = request.map(ask) { post(Posted::Asked(key.to_owned(), ask)); }
    if focus { post(Posted::Asked(key.to_owned(), Ask::Focus)); }
}

pub(crate) fn drawn(key: &str) { post(Posted::Drawn(key.to_owned())); }

pub(crate) fn connected(scene: &AnyObject, key: Option<String>) {
    post(Posted::Connected { scene: scene.retain(), key });
}

pub(crate) fn disconnected(scene: &AnyObject) { post(Posted::Disconnected(scene.retain())); }

pub(crate) fn foreground(scene: &AnyObject) { post(Posted::Foreground(scene.retain(), scene_session_key(scene))); }

fn scene_session_key(scene: &AnyObject) -> Option<String> {
    // SAFETY: a scene's `session` is never nil.
    let session: Retained<AnyObject> = unsafe { msg_send![scene, session] };
    session_key(&session)
}

pub(crate) fn background(scene: &AnyObject) { post(Posted::Background(scene.retain())); }

pub(crate) fn discarded(key: String) { post(Posted::Discarded(key)); }

fn ask(request: WindowRequest) -> Ask {
    match request {
        WindowRequest::Open(_) | WindowRequest::Show => Ask::Show,
        WindowRequest::Hide => Ask::Hide,
        WindowRequest::Close => Ask::Close,
    }
}

pub(crate) fn start(entries: Vec<Entry>) { PENDING.set(Some(entries)); }

fn launch(mtm: MainThreadMarker) {
    let Some(entries) = PENDING.take() else { return };
    let mode = if several_scenes(mtm) { Mode::Scenes } else { Mode::Sheets };
    let mut list = Vec::with_capacity(entries.len());
    let mut slots = Vec::with_capacity(entries.len());
    for (at, entry) in entries.into_iter().enumerate() {
        list.push(Window { key: entry.key.clone(), starts_visible: at == 0 || entry.starts_visible, state: State::Hidden });
        slots.push(slot(entry));
    }
    REGISTRY.set(Some(Registry { windows: Windows::new(mode, list), slots, scenes: Vec::new(), refused: false }));
}

fn slot(entry: Entry) -> Slot {
    let Entry { key, mount, requests, handle, .. } = entry;
    requests.connect(pump_wake());
    if let Some(handle) = &handle { handle.connect_ios(pump_wake()); }
    let mount: Mount = Box::new(move |layer, size, scale, dark, wake, presents| {
        let both: Waker = Arc::new(move || { wake(); schedule(); });
        let mut surface = mount(layer, size, scale, dark, both, presents)?;
        surface.set_window(key);
        Ok(surface)
    });
    Slot { mount: Some(mount), view: None, controller: None, sheet: None, window: None, requests, handle }
}

fn several_scenes(_: MainThreadMarker) -> bool {
    // SAFETY: the shared application and current device are singletons read on the main
    // thread; `supportsMultipleScenes` is a `BOOL` and `userInterfaceIdiom` an `NSInteger`.
    unsafe {
        let app: Retained<AnyObject> = msg_send![class!(UIApplication), sharedApplication];
        let several: bool = msg_send![&*app, supportsMultipleScenes];
        let device: Retained<AnyObject> = msg_send![class!(UIDevice), currentDevice];
        let idiom: isize = msg_send![&*device, userInterfaceIdiom];
        several && matches!(idiom, 1 | 5)
    }
}

fn pump() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    launch(mtm);
    if REGISTRY.with_borrow(Option::is_none) { return; }
    gather();
    while let Some(posted) = QUEUE.with_borrow_mut(VecDeque::pop_front) {
        let effects = REGISTRY.with_borrow_mut(|registry| {
            let registry = registry.as_mut()?;
            let event = registry.event(&posted)?;
            Some(windows::step(&mut registry.windows, event).into_iter().collect::<Vec<_>>())
        });
        let Some(effects) = effects else { continue };
        let last = effects.len();
        for (at, effect) in effects.iter().enumerate() {
            let presents_next = effects[at + 1..last].iter().any(|e| matches!(e, Effect::Present(_)));
            apply(mtm, *effect, presents_next);
        }
        gather();
    }
}

fn gather() {
    let opened = crate::windows::take_opened();
    REGISTRY.with_borrow_mut(|registry| {
        let Some(registry) = registry.as_mut() else { return };
        for spec in opened {
            if registry.windows.find(spec.key()).is_some() { continue; }
            let entry = spec.entry();
            let key = entry.key.clone();
            registry.windows.push(key.clone());
            registry.slots.push(slot(entry));
            QUEUE.with_borrow_mut(|queue| queue.push_back(Posted::Asked(key, Ask::Show)));
        }
        for at in 0..registry.slots.len() {
            let key = registry.windows.key(at).to_owned();
            let (request, focus) = registry.slots[at].requests.take_window_asks();
            let mut asks: Vec<Ask> = request.map(ask).into_iter().chain(focus.then_some(Ask::Focus)).collect();
            if let Some(handle) = &registry.slots[at].handle {
                for event in handle.take_headless() {
                    match event {
                        crate::window::WakeEvent::Window(_, request) => asks.push(ask(request)),
                        crate::window::WakeEvent::Focus(_) => asks.push(Ask::Focus),
                        _ => {}
                    }
                }
            }
            QUEUE.with_borrow_mut(|queue| queue.extend(asks.into_iter().map(|ask| Posted::Asked(key.clone(), ask))));
        }
    });
}

impl Registry {
    fn scene_id(&mut self, scene: &AnyObject) -> Scene {
        if let Some((id, _)) = self.scenes.iter().find(|(_, s)| std::ptr::eq(&**s, scene)) { return *id; }
        let id = NEXT_SCENE.get();
        NEXT_SCENE.set(id + 1);
        self.scenes.push((id, scene.retain()));
        id
    }

    fn event<'a>(&mut self, posted: &'a Posted) -> Option<Event<'a>> {
        Some(match posted {
            Posted::Connected { scene, key } => Event::Connected { scene: self.scene_id(scene), key: key.as_deref() },
            Posted::Disconnected(scene) => {
                let id = self.scene_id(scene);
                self.scenes.retain(|(s, _)| *s != id);
                Event::Disconnected(id)
            }
            Posted::Foreground(scene, key) => Event::Foreground { scene: self.scene_id(scene), key: key.as_deref() },
            Posted::Background(scene) => Event::Background(self.scene_id(scene)),
            Posted::Discarded(key) => Event::Discarded(key),
            Posted::Asked(key, ask) => Event::Ask { window: self.windows.find(key)?, ask: *ask },
            Posted::Dismissed(key) => Event::Dismissed(self.windows.find(key)?),
            Posted::Drawn(key) if self.windows.find(key) == Some(0) => Event::Drawn,
            Posted::Drawn(_) => return None,
        })
    }

    fn scene(&self, id: Scene) -> Option<Retained<AnyObject>> {
        self.scenes.iter().find(|(s, _)| *s == id).map(|(_, scene)| scene.clone())
    }
}

fn with_slot<R>(window: usize, f: impl FnOnce(&mut Slot, &str) -> R) -> Option<R> {
    REGISTRY.with_borrow_mut(|registry| {
        let registry = registry.as_mut()?;
        let key = registry.windows.key(window).to_owned();
        Some(f(&mut registry.slots[window], &key))
    })
}

fn controller(mtm: MainThreadMarker, slot: &mut Slot) -> Retained<RuneViewController> {
    if let Some(controller) = &slot.controller { return controller.clone(); }
    let view = match &slot.view {
        Some(view) => view.clone(),
        None => {
            let mount = slot.mount.take().expect("a window's mount waits for its first show");
            let view = RuneView::new(mtm, mount);
            slot.view = Some(view.clone());
            view
        }
    };
    let controller = RuneViewController::new(mtm);
    // SAFETY: a view controller's `view` takes a `UIView`, which a `RuneView` is.
    let _: () = unsafe { msg_send![&*controller, setView: &*view] };
    slot.controller = Some(controller.clone());
    controller
}

fn apply(mtm: MainThreadMarker, effect: Effect, presents_next: bool) {
    match effect {
        Effect::Attach { window, scene } => attach(mtm, window, scene),
        Effect::Activate(window) => {
            let Some(key) = with_slot(window, |_, key| key.to_owned()) else { return };
            activate(&key);
        }
        Effect::Destroy(window) => {
            let Some(key) = with_slot(window, |slot, key| {
                if let Some(old) = slot.window.take() {
                    // SAFETY: a window's `rootViewController` is nullable.
                    let _: () = unsafe { msg_send![&*old, setRootViewController: None::<&AnyObject>] };
                }
                key.to_owned()
            }) else { return };
            if let Some(session) = session(&key) { destroy(&session); }
        }
        Effect::Refuse(scene) => {
            let scene = REGISTRY.with_borrow(|registry| registry.as_ref().and_then(|r| r.scene(scene)));
            if let Some(scene) = scene {
                // SAFETY: a scene's `session` is never nil.
                let session: Retained<AnyObject> = unsafe { msg_send![&*scene, session] };
                destroy(&session);
            }
        }
        Effect::Present(window) => present(mtm, window),
        Effect::Dismiss(window) => {
            let controller = with_slot(window, |slot, _| slot.controller.clone()).flatten();
            // SAFETY: a presented controller dismisses itself and those above it.
            if let Some(controller) = controller { unsafe { let _: () = msg_send![&*controller, dismissViewControllerAnimated: !presents_next, completion: None::<&AnyObject>]; } }
        }
        Effect::Closed(window) => {
            let view = with_slot(window, |slot, _| slot.view.clone()).flatten();
            if let Some(view) = view { view.with_surface(|surface| surface.close()); }
        }
        Effect::Refused(window) => {
            let first = REGISTRY.with_borrow_mut(|registry| registry.as_mut().is_some_and(|r| !std::mem::replace(&mut r.refused, true)));
            if first { eprintln!("rune: hiding or closing window {window} ignored: an iOS app with one scene always shows its first window"); }
        }
    }
}

fn attach(mtm: MainThreadMarker, window: usize, scene_id: Scene) {
    let Some(scene) = REGISTRY.with_borrow(|registry| registry.as_ref().and_then(|r| r.scene(scene_id))) else { return };
    let Some((controller, view, key, old)) = with_slot(window, |slot, key| {
        let controller = controller(mtm, slot);
        (controller, slot.view.clone(), key.to_owned(), slot.window.take())
    }) else { return };
    // SAFETY: a window's `rootViewController` is nullable; the scene of a window-scene session
    // is a `UIWindowScene`; a session's `userInfo` takes a property-list dictionary.
    let ui_window = unsafe {
        if let Some(old) = old { let _: () = msg_send![&*old, setRootViewController: None::<&AnyObject>]; }
        let ui_window: Allocated<AnyObject> = msg_send![class!(UIWindow), alloc];
        let ui_window: Retained<AnyObject> = msg_send![ui_window, initWithWindowScene: &*scene];
        let _: () = msg_send![&*ui_window, setRootViewController: &*controller];
        let _: () = msg_send![&*ui_window, makeKeyAndVisible];
        if let Some(view) = &view { let _: bool = msg_send![&**view, becomeFirstResponder]; }
        let session: Retained<AnyObject> = msg_send![&*scene, session];
        let info: Retained<AnyObject> = msg_send![class!(NSDictionary), dictionaryWithObject: &*NSString::from_str(&key), forKey: &*NSString::from_str(KEY)];
        let _: () = msg_send![&*session, setUserInfo: &*info];
        ui_window
    };
    with_slot(window, |slot, _| slot.window = Some(ui_window));
}

fn present(mtm: MainThreadMarker, window: usize) {
    let Some(root) = with_slot(0, |slot, _| slot.controller.clone()).flatten() else { return };
    let Some((controller, sheet)) = with_slot(window, |slot, key| {
        let controller = controller(mtm, slot);
        let sheet = slot.sheet.get_or_insert_with(|| RuneSheet::new(mtm, key)).clone();
        (controller, sheet)
    }) else { return };
    // SAFETY: `presentedViewController` is nullable; `modalPresentationStyle` takes a
    // `UIModalPresentationStyle`; a presentation controller's delegate is a weak object
    // conforming to the protocol `RuneSheet` implements, which the slot keeps alive.
    unsafe {
        let mut top: Retained<AnyObject> = Retained::cast_unchecked(root);
        loop {
            let next: Option<Retained<AnyObject>> = msg_send![&*top, presentedViewController];
            match next {
                Some(next) => top = next,
                None => break,
            }
        }
        let _: () = msg_send![&*controller, setModalPresentationStyle: PAGE_SHEET];
        let presentation: Retained<AnyObject> = msg_send![&*controller, presentationController];
        let _: () = msg_send![&*presentation, setDelegate: &*sheet];
        let _: () = msg_send![&*top, presentViewController: &*controller, animated: true, completion: None::<&AnyObject>];
    }
}

fn session(key: &str) -> Option<Retained<AnyObject>> {
    // SAFETY: `openSessions` is an `NSSet<UISceneSession *>`, whose members' `userInfo` is a
    // nullable dictionary of strings.
    unsafe {
        let app: Retained<AnyObject> = msg_send![class!(UIApplication), sharedApplication];
        let sessions: Retained<AnyObject> = msg_send![&*app, openSessions];
        super::app::members(&sessions).into_iter().find(|session| session_key(session).as_deref() == Some(key))
    }
}

pub(crate) fn session_key(session: &AnyObject) -> Option<String> {
    // SAFETY: a session's `userInfo` is a nullable dictionary; `objectForKey:` answers an object
    // or nil, a string where Rune put one.
    unsafe {
        let info: Option<Retained<AnyObject>> = msg_send![session, userInfo];
        let value: Option<Retained<AnyObject>> = msg_send![&*info?, objectForKey: &*NSString::from_str(KEY)];
        let value = value?;
        let string: bool = msg_send![&*value, isKindOfClass: class!(NSString)];
        string.then(|| Retained::cast_unchecked::<NSString>(value).to_string())
    }
}

fn activate(key: &str) {
    let existing = session(key);
    // SAFETY: `requestSceneSessionActivation:userActivity:options:errorHandler:` takes a
    // nullable session, activity, options and block taking an `NSError`, which UIKit copies, so
    // the handler outlives this call; an activity is made with its type and carries a
    // property-list `userInfo`.
    unsafe {
        let activity: Allocated<AnyObject> = msg_send![class!(NSUserActivity), alloc];
        let activity: Retained<AnyObject> = msg_send![activity, initWithActivityType: &*NSString::from_str(ACTIVITY)];
        let info: Retained<AnyObject> = msg_send![class!(NSDictionary), dictionaryWithObject: &*NSString::from_str(key), forKey: &*NSString::from_str(KEY)];
        let _: () = msg_send![&*activity, setUserInfo: &*info];
        let app: Retained<AnyObject> = msg_send![class!(UIApplication), sharedApplication];
        let failed = failure(format!("showing window {key:?}"));
        let _: () = msg_send![&*app, requestSceneSessionActivation: existing.as_deref(), userActivity: &*activity, options: None::<&AnyObject>, errorHandler: &*failed];
    }
}

fn destroy(session: &AnyObject) {
    let failed = failure(format!("ending the scene of window {:?}", session_key(session).unwrap_or_default()));
    // SAFETY: `requestSceneSessionDestruction:options:errorHandler:` takes a session, nullable
    // options and a nullable block taking an `NSError`, which UIKit copies, so the handler
    // outlives this call.
    unsafe {
        let app: Retained<AnyObject> = msg_send![class!(UIApplication), sharedApplication];
        let _: () = msg_send![&*app, requestSceneSessionDestruction: session, options: None::<&AnyObject>, errorHandler: &*failed];
    }
}

fn failure(what: String) -> RcBlock<dyn Fn(NonNull<AnyObject>)> {
    RcBlock::new(move |error: NonNull<AnyObject>| {
        guarded("scene request error handler", || {
            // SAFETY: the handler is given an `NSError`, whose `localizedDescription` is a string.
            let why: Retained<NSString> = unsafe { msg_send![error.as_ref(), localizedDescription] };
            eprintln!("rune: {what} failed: {why}");
        })
    })
}

pub(crate) fn activity_key(activity: &AnyObject) -> Option<String> {
    // SAFETY: an activity's `activityType` is a string and its `userInfo` a nullable dictionary.
    unsafe {
        let kind: Retained<NSString> = msg_send![activity, activityType];
        if kind.to_string() != ACTIVITY { return None; }
        let info: Option<Retained<AnyObject>> = msg_send![activity, userInfo];
        let value: Option<Retained<AnyObject>> = msg_send![&*info?, objectForKey: &*NSString::from_str(KEY)];
        let value = value?;
        let string: bool = msg_send![&*value, isKindOfClass: class!(NSString)];
        string.then(|| Retained::cast_unchecked::<NSString>(value).to_string())
    }
}
