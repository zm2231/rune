//! VoiceOver, Switch Control and Voice Control for a `RuneView`. The view is an accessibility
//! container: its elements are `RuneAccessibilityElement`s, one for each element of the
//! mirror (`a11y::ios`), which flattens the runtime's accessibility tree after each frame.
//! Until a client asks for the elements, or VoiceOver is already running when the view joins a
//! window, the runtime builds no tree.
//!
//! An element lives for as long as its tree node does, so a reader's cursor stays on a control
//! whose label or value changed, and what a reader does to it (a double tap, a swipe on a
//! slider, a three-finger scroll) goes back to the runtime as the tree's own action.

#![allow(unsafe_code)]

mod field;

use std::cell::{Cell, RefCell};

use accesskit::ActionRequest;
use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_foundation::NSString;
use objc2::rc::{Allocated, Retained, Weak};
use objc2::runtime::{AnyObject, AnyProtocol, NSObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, class, define_class, extern_class, msg_send, sel};

use super::surface::Reading;
use super::text::input::RuneTextInput;
use super::view::RuneView;
use crate::platform::objc_boundary::guarded;
use crate::a11y::ios::{Element, Gesture, Mirror, Toward, traits};
use crate::runtime::HashMap;

// SAFETY: the declarations match UIKit's headers: a `UIAccessibilityTraits` is a `uint64_t`, a
// `UIAccessibilityNotifications` a `uint32_t`, and the functions have these C signatures.
#[link(name = "UIKit", kind = "framework")]
unsafe extern "C" {
    static UIAccessibilityTraitButton: u64;
    static UIAccessibilityTraitLink: u64;
    static UIAccessibilityTraitImage: u64;
    static UIAccessibilityTraitSelected: u64;
    static UIAccessibilityTraitStaticText: u64;
    static UIAccessibilityTraitNotEnabled: u64;
    static UIAccessibilityTraitAdjustable: u64;
    static UIAccessibilityTraitHeader: u64;
    static UIAccessibilityTraitSearchField: u64;
    static UIAccessibilityTraitUpdatesFrequently: u64;
    static UIAccessibilityLayoutChangedNotification: u32;
    static UIAccessibilityScreenChangedNotification: u32;
    static UIAccessibilityAnnouncementNotification: u32;
    static UIAccessibilityPageScrolledNotification: u32;
    fn UIAccessibilityPostNotification(notification: u32, argument: Option<&AnyObject>);
    fn UIAccessibilityIsVoiceOverRunning() -> bool;
}

/// `UIAccessibilityTraitToggleButton`, which iOS 17 added: looked up by name so an older
/// system, which has no such trait, still loads the app.
fn toggle_trait() -> u64 {
    // SAFETY: the symbol, where it exists, is a `UIAccessibilityTraits` constant.
    unsafe {
        let symbol = libc::dlsym(libc::RTLD_DEFAULT, c"UIAccessibilityTraitToggleButton".as_ptr());
        if symbol.is_null() { 0 } else { *symbol.cast::<u64>() }
    }
}

/// The mirror's traits as UIKit's.
fn ui_traits(mtm: MainThreadMarker, bits: u16) -> u64 {
    // SAFETY: constants UIKit exports.
    let map = unsafe {
        [
            (traits::BUTTON, UIAccessibilityTraitButton),
            (traits::LINK, UIAccessibilityTraitLink),
            (traits::IMAGE, UIAccessibilityTraitImage),
            (traits::SELECTED, UIAccessibilityTraitSelected),
            (traits::STATIC_TEXT, UIAccessibilityTraitStaticText),
            (traits::DISABLED, UIAccessibilityTraitNotEnabled),
            (traits::ADJUSTABLE, UIAccessibilityTraitAdjustable),
            (traits::HEADER, UIAccessibilityTraitHeader),
            (traits::SEARCH, UIAccessibilityTraitSearchField),
            (traits::UPDATES, UIAccessibilityTraitUpdatesFrequently),
            (traits::TOGGLE, toggle_trait()),
            (traits::TEXT_FIELD, field::field_traits(mtm, false)),
            (traits::SECURE_TEXT_FIELD, field::field_traits(mtm, true)),
        ]
    };
    map.iter().filter(|(bit, _)| bits & bit != 0).fold(0, |all, (_, ui)| all | ui)
}

/// `UIAccessibilityScrollDirection`: which way the reader's three fingers moved. The content
/// follows the fingers, so what shows next lies the other way.
fn toward(direction: isize) -> Option<Toward> {
    Some(match direction {
        1 => Toward::Left,
        2 => Toward::Right,
        3 => Toward::Down,
        4 | 6 => Toward::Up,
        5 => Toward::Down,
        _ => return None,
    })
}

extern_class!(
    // SAFETY: UIKit's class of this name is an `NSObject`, through `UIResponder`: naming the
    // root alone claims no method the class lacks.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "UIAccessibilityElement"]
    struct UIAccessibilityElement;
);

struct ElementIvars {
    id: Cell<u64>,
    host: Weak<RuneView>,
    expanded: Cell<isize>,
    menu: Cell<bool>,
    editing: Cell<bool>,
    actions: RefCell<Option<Retained<AnyObject>>>,
}

define_class!(
    // SAFETY: `UIAccessibilityElement` is made to be subclassed, and the element has no `Drop`.
    #[unsafe(super(UIAccessibilityElement, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RuneAccessibilityElement"]
    #[ivars = ElementIvars]
    struct RuneElement;

    impl RuneElement {
        #[unsafe(method(accessibilityActivate))]
        fn activate(&self) -> bool {
            guarded("accessibilityActivate", || self.did(Gesture::Activate))
        }

        #[unsafe(method(accessibilityIncrement))]
        fn increment(&self) {
            guarded("accessibilityIncrement", || { self.did(Gesture::Increment); })
        }

        #[unsafe(method(accessibilityDecrement))]
        fn decrement(&self) {
            guarded("accessibilityDecrement", || { self.did(Gesture::Decrement); })
        }

        #[unsafe(method(accessibilityElementDidBecomeFocused))]
        fn focused(&self) {
            guarded("accessibilityElementDidBecomeFocused", || { self.did(Gesture::Focused); })
        }

        #[unsafe(method(accessibilityScroll:))]
        fn scroll(&self, direction: isize) -> bool {
            guarded("accessibilityScroll:", || toward(direction).is_some_and(|toward| self.did(Gesture::Scroll(toward))))
        }

        #[unsafe(method(isAccessibilityElement))]
        fn is_element(&self) -> bool {
            guarded("isAccessibilityElement", || true)
        }

        #[unsafe(method_id(accessibilityCustomActions))]
        fn custom_actions(&self) -> Option<Retained<AnyObject>> {
            guarded("accessibilityCustomActions", || self.ivars().actions.borrow().clone())
        }

        #[unsafe(method(accessibilityExpandedStatus))]
        fn expanded_status(&self) -> isize {
            guarded("accessibilityExpandedStatus", || self.ivars().expanded.get())
        }

        #[unsafe(method(respondsToSelector:))]
        fn responds(&self, sel: Sel) -> bool {
            guarded("respondsToSelector:", || {
                // SAFETY: `NSObject`'s method of this signature.
                let own: bool = unsafe { msg_send![super(self), respondsToSelector: sel] };
                own || self.text_input(sel).is_some()
            })
        }

        #[unsafe(method(conformsToProtocol:))]
        fn conforms(&self, protocol: &AnyProtocol) -> bool {
            guarded("conformsToProtocol:", || {
                // SAFETY: `NSObject`'s method of this signature.
                let own: bool = unsafe { msg_send![super(self), conformsToProtocol: protocol] };
                own || (field::is_text_protocol(protocol) && self.editing_input().is_some())
            })
        }

        #[unsafe(method(forwardingTargetForSelector:))]
        fn forward(&self, sel: Sel) -> *mut AnyObject {
            guarded("forwardingTargetForSelector:", || {
                match self.text_input(sel) {
                    // The view holds the input, so it outlives the call it is forwarded.
                    Some(input) => Retained::as_ptr(&input).cast_mut().cast(),
                    // SAFETY: `NSObject`'s method of this signature.
                    None => unsafe { msg_send![super(self), forwardingTargetForSelector: sel] },
                }
            })
        }

        #[unsafe(method(runeShowMenu:))]
        fn show_menu(&self, _action: &AnyObject) -> bool {
            guarded("runeShowMenu:", || self.did(Gesture::Menu))
        }
    }
);

impl RuneElement {
    fn new(mtm: MainThreadMarker, host: &RuneView, id: u64) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ElementIvars { id: Cell::new(id), host: Weak::new(host), expanded: Cell::new(0), menu: Cell::new(false), editing: Cell::new(false), actions: RefCell::new(None) });
        // SAFETY: `initWithAccessibilityContainer:` is the designated initializer, and the
        // container is the view the element belongs to.
        unsafe { msg_send![super(this), initWithAccessibilityContainer: host] }
    }

    /// Sends the host what the reader did: true when the element took it.
    fn did(&self, gesture: Gesture) -> bool {
        let ivars = self.ivars();
        ivars.host.load().is_some_and(|host| host.accessibility_gesture(ivars.id.get(), gesture))
    }

    /// The view's text input, while this element's field is edited through it and is not a
    /// password.
    fn editing_input(&self) -> Option<Retained<RuneTextInput>> {
        if !self.ivars().editing.get() { return None }
        self.ivars().host.load()?.readable_input()
    }

    /// The text input that answers `sel` for this element, when `sel` is a text input method.
    fn text_input(&self, sel: Sel) -> Option<Retained<RuneTextInput>> {
        if !self.ivars().editing.get() || !field::forwards(sel) { return None }
        let input = self.editing_input()?;
        // SAFETY: `NSObject`'s method of this signature.
        let answers: bool = unsafe { msg_send![&*input, respondsToSelector: sel] };
        answers.then_some(input)
    }

    /// Says what `element` says now. `zoom` scales its rect to the view's points.
    fn describe(&self, element: &Element, zoom: f32) {
        let text = |s: Option<&str>| s.map(NSString::from_str);
        let [x, y, w, h] = super::zoom::to_window(zoom, element.rect).map(|v| v as CGFloat);
        // SAFETY: `UIAccessibilityElement`'s properties, of these types.
        unsafe {
            let _: () = msg_send![self, setAccessibilityLabel: &*NSString::from_str(&element.label)];
            let _: () = msg_send![self, setAccessibilityValue: text(element.value.as_deref()).as_deref()];
            let _: () = msg_send![self, setAccessibilityHint: text(element.hint.as_deref()).as_deref()];
            let _: () = msg_send![self, setAccessibilityTraits: ui_traits(self.mtm(), element.traits)];
            let _: () = msg_send![self, setAccessibilityFrameInContainerSpace: CGRect::new(CGPoint::new(x, y), CGSize::new(w, h))];
        }
        self.ivars().editing.set(element.editing);
        self.ivars().expanded.set(match element.expanded { None => 0, Some(true) => 1, Some(false) => 2 });
        if self.ivars().menu.replace(element.menu) != element.menu {
            self.set_menu(element.menu);
        }
    }

    fn set_menu(&self, menu: bool) {
        // SAFETY: `initWithName:target:selector:` takes a name, a target it holds weakly and a
        // selector the target answers with a `BOOL`, as `runeShowMenu:` does; `arrayWithObject:`
        // takes an object that is not nil.
        let actions = menu.then(|| unsafe {
            let name = NSString::from_str("Show menu");
            let action: Allocated<AnyObject> = msg_send![class!(UIAccessibilityCustomAction), alloc];
            let action: Retained<AnyObject> = msg_send![action, initWithName: &*name, target: self, selector: sel!(runeShowMenu:)];
            msg_send![class!(NSArray), arrayWithObject: &*action]
        });
        *self.ivars().actions.borrow_mut() = actions;
    }
}

/// A view's accessibility: the mirror, and the elements UIKit holds for it.
pub(crate) struct Accessibility {
    mirror: Mirror,
    made: HashMap<u64, Retained<RuneElement>>,
    /// The `NSArray` the view answers `accessibilityElements` with.
    list: Retained<AnyObject>,
    /// A client asked: the runtime builds its tree.
    asked: bool,
}

impl Accessibility {
    pub(crate) fn new() -> Self {
        // SAFETY: `+[NSArray array]` takes nothing and returns a new array, never nil.
        let list = unsafe { msg_send![class!(NSArray), array] };
        Accessibility { mirror: Mirror::default(), made: HashMap::default(), list, asked: false }
    }

    /// Whether VoiceOver runs now: a view that joins a window then builds its tree at once.
    pub(crate) fn reader_running() -> bool {
        // SAFETY: a C function that reads a setting.
        unsafe { UIAccessibilityIsVoiceOverRunning() }
    }

    pub(crate) fn ask(&mut self) -> bool {
        !std::mem::replace(&mut self.asked, true)
    }

    pub(crate) fn list(&self) -> Retained<AnyObject> { self.list.clone() }

    pub(crate) fn answer(&mut self, mtm: MainThreadMarker, host: &RuneView, reading: Reading) -> Posts {
        let posts = self.apply(mtm, host, reading);
        match posts.start {
            Some(start) if posts.screen => Posts { screen: true, start: Some(start), ..Posts::default() },
            _ => Posts::default(),
        }
    }

    /// Whether a client asked for the elements yet.
    pub(crate) fn asked(&self) -> bool { self.asked }

    pub(crate) fn request(&mut self, id: u64, gesture: Gesture) -> Option<ActionRequest> {
        self.mirror.request(id, gesture)
    }

    /// Takes the tree's changes: elements are made, described again or dropped, and the
    /// reader is told what moved and what was said. Returns what to post once the view's own
    /// state is no longer borrowed.
    pub(crate) fn apply(&mut self, mtm: MainThreadMarker, host: &RuneView, reading: Reading) -> Posts {
        let first = self.mirror.elements().is_empty();
        let masked = reading.masks.is_some_and(|masks| self.mirror.set_masks(masks));
        let changes = match reading.tree {
            Some(update) => {
                self.mirror.set_start(reading.first);
                self.mirror.apply(update)
            }
            None if masked => self.mirror.refresh(),
            None => return Posts::default(),
        };
        let zoom = reading.zoom;
        let mut made = HashMap::default();
        let landing = self.mirror.landing();
        let mut start = None;
        // SAFETY: `+[NSMutableArray array]` takes nothing and returns a new array, never nil.
        let list: Retained<AnyObject> = unsafe { msg_send![class!(NSMutableArray), array] };
        for element in self.mirror.elements() {
            if let Some(slot) = element.hosted.and_then(crate::native_view::slot) {
                if let Some(view) = slot.view.object() {
                    // SAFETY: `addObject:` takes an object that is not nil, which the hosted view is.
                    let _: () = unsafe { msg_send![&*list, addObject: view] };
                    if landing == Some(element.id) { start = Some(retain(view)); }
                }
                continue;
            }
            let object = self.made.remove(&element.id).unwrap_or_else(|| RuneElement::new(mtm, host, element.id));
            object.describe(element, zoom);
            // SAFETY: `addObject:` takes an object that is not nil, which the element is.
            let _: () = unsafe { msg_send![&*list, addObject: &*object] };
            if landing == Some(element.id) { start = Some(retain(&object)); }
            made.insert(element.id, object);
        }
        let focus = changes.focus.and_then(|id| made.get(&id).cloned());
        let screen = first && changes.layout;
        let start = start.filter(|_| screen);
        self.made = made;
        self.list = list;
        let layout = !first && changes.layout && changes.page.is_none();
        Posts { screen, start, layout, focus, page: changes.page, announce: changes.announce }
    }
}

fn retain(object: &AnyObject) -> Retained<AnyObject> {
    // SAFETY: `object` is a live Objective-C object borrowed for this call, so retaining it is
    // sound and the pointer is never null.
    unsafe { Retained::retain(std::ptr::from_ref(object).cast_mut()) }.expect("a borrowed object is not null")
}

/// What to tell the reader after an update.
#[derive(Default)]
pub(crate) struct Posts {
    screen: bool,
    start: Option<Retained<AnyObject>>,
    layout: bool,
    focus: Option<Retained<RuneElement>>,
    page: Option<String>,
    announce: Vec<String>,
}

impl Posts {
    pub(crate) fn post(self) {
        // SAFETY: UIKit's notifications, each with the argument it documents: none to let
        // the reader choose where to land, the element to land on, or the words to speak.
        unsafe {
            if self.screen { UIAccessibilityPostNotification(UIAccessibilityScreenChangedNotification, self.start.as_deref()); }
            match &self.focus {
                Some(element) => UIAccessibilityPostNotification(UIAccessibilityLayoutChangedNotification, Some(element)),
                None if self.layout => UIAccessibilityPostNotification(UIAccessibilityLayoutChangedNotification, None),
                None => {}
            }
            if let Some(page) = &self.page {
                UIAccessibilityPostNotification(UIAccessibilityPageScrolledNotification, Some(&NSString::from_str(page)));
            }
            for words in &self.announce {
                UIAccessibilityPostNotification(UIAccessibilityAnnouncementNotification, Some(&NSString::from_str(words)));
            }
        }
    }
}
