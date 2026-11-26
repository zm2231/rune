//! A `RuneView`'s pop-up menus, and the long press that asks for one. A finger held still on
//! something with a menu (a text field's Cut, Copy and Paste, selectable text's Copy, an
//! element's `on_secondary_press`) is the secondary press a right-click is on a desktop. The
//! menu a handler then asks for (`Tx::popup_menu`) shows as the system's edit menu, the bar
//! above a selection, through a `UIEditMenuInteraction`: its items are `UICommand`s that come
//! back to the view through the responder chain, so nothing here needs a block.

#![allow(unsafe_code)]

use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_foundation::NSString;
use objc2::rc::{Allocated, Retained, Weak};
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, class, msg_send, sel};

use super::super::pacing::Event;
use super::super::edit_menu::{self, Effect};
use super::{MainOnly, RuneView};
use crate::machine::Attempt;
use crate::platform::main_queue::after_main;
use crate::popup_menu::{PopupEntry, PopupMenu, Show};
use crate::runtime::TouchPhase;

/// `UIGestureRecognizerStateBegan`, `Changed` and `Ended`.
const BEGAN: isize = 1;
const CHANGED: isize = 2;
const ENDED: isize = 3;
/// `UIMenuElementAttributesDisabled`.
const DISABLED: usize = 1 << 0;
/// `UIMenuElementStateOn`.
const ON: isize = 1;
/// `UIMenuOptionsDisplayInline`: the children show in the parent, set apart, with no title.
const INLINE: usize = 1 << 0;

impl RuneView {
    /// Whether a long press at the recognizer's place would open anything. One that wouldn't
    /// never begins, so a finger resting on a list or a slider keeps its touch.
    pub(super) fn long_press_begins(&self, recognizer: &AnyObject) -> bool {
        // SAFETY: a recognizer's location in a view is a `CGPoint`.
        let at: CGPoint = unsafe { msg_send![recognizer, locationInView: self] };
        if self.ivars().text.borrow().as_ref().is_some_and(|text| text.dragging_handle()) { return false; }
        self.with_surface(|surface| surface.takes_secondary(at.x as f32, at.y as f32)).unwrap_or(false)
    }

    /// The long press began: UIKit has cancelled the finger's touch. On the focused field's
    /// text the caret follows the finger under the loupe until it lifts; elsewhere the runtime
    /// hears a secondary press where it rests.
    pub(super) fn long_pressed(&self, recognizer: &AnyObject) {
        // SAFETY: a recognizer's `state` is an `NSInteger` enum and its location a `CGPoint`.
        let (state, at): (isize, CGPoint) = unsafe { (msg_send![recognizer, state], msg_send![recognizer, locationInView: self]) };
        let at = (at.x as f32, at.y as f32);
        let input = self.ivars().text.borrow().clone();
        let phase = match state {
            BEGAN => TouchPhase::Started,
            CHANGED => TouchPhase::Moved,
            ENDED => TouchPhase::Ended,
            _ => TouchPhase::Cancelled,
        };
        if phase != TouchPhase::Started {
            if let Some(input) = input { input.hold(phase, at); }
            return;
        }
        let on_field = self.with_surface(|surface| surface.on_focused_field(at.0, at.1)).unwrap_or(false);
        if on_field && input.is_some_and(|input| input.hold(phase, at)) { return; }
        self.with_surface(|surface| surface.secondary(at.0, at.1));
        self.pace(Event::Wake);
    }

    /// Puts `show`'s menu up, at the focused field's caret when it is the field's own menu.
    pub(super) fn open_menu(&self, show: Show, field: bool) {
        let caret = field.then(|| self.caret()).flatten();
        self.menu_step(edit_menu::Event::Present { show, caret });
    }

    /// The open menu as a `UIMenu`, for the interaction.
    pub(super) fn menu_items(&self) -> Option<Retained<AnyObject>> {
        self.ivars().menu.borrow().show().map(|show| build(&show.menu, "", 0))
    }

    /// What the open menu points at.
    pub(super) fn menu_target(&self) -> CGRect {
        let [x, y, w, h] = self.ivars().menu.borrow().target();
        CGRect::new(CGPoint::new(x, y), CGSize::new(w, h))
    }

    /// After a frame: a field's menu that is up moves with the caret.
    pub(super) fn menu_follow_caret(&self) {
        if !self.ivars().menu.borrow().follows_caret() { return; }
        if let Some(caret) = self.caret() { self.menu_step(edit_menu::Event::Caret(caret)); }
    }

    pub(crate) fn hide_menu(&self) {
        let interaction = self.ivars().edit_menu.borrow().clone();
        // SAFETY: `UIEditMenuInteraction`'s `dismissMenu` takes nothing.
        if let Some(interaction) = interaction { unsafe { let _: () = msg_send![&*interaction, dismissMenu]; } }
    }

    /// What the focused field's menu points at, where the keyboard's text input has it.
    fn caret(&self) -> Option<edit_menu::Rect> {
        let rect = self.ivars().text.borrow().as_ref().map(|input| input.menu_anchor()).filter(|rect| rect.size.height > 0.0)?;
        Some([rect.origin.x, rect.origin.y, rect.size.width, rect.size.height])
    }

    /// An item was picked: `command` is its `UICommand`, whose property list is the item's id.
    pub(super) fn menu_picked(&self, command: &AnyObject) {
        // SAFETY: the commands `build` makes carry an `NSString`.
        let id: Option<Retained<NSString>> = unsafe { msg_send![command, propertyList] };
        self.menu_step(edit_menu::Event::Picked(id.map(|id| id.to_string())));
    }

    /// The menu is going away. A tapped item's action follows shortly; with none, the runtime
    /// hears the menu was dismissed.
    pub(super) fn menu_dismissed(&self) {
        self.menu_step(edit_menu::Event::Dismissed);
    }

    /// The grace after a dismissal ran out: unless a pick or another menu came first, nothing
    /// was picked.
    pub(super) fn menu_left(&self, attempt: Attempt) {
        self.menu_step(edit_menu::Event::Timer(attempt));
    }

    /// Steps the menu and carries out what the step asks for. The menu's borrow ends before
    /// the effects run: presenting has the interaction ask the view for its items and place.
    fn menu_step(&self, event: edit_menu::Event) {
        let ivars = self.ivars();
        let mut seq = ivars.menu_seq.get();
        let effects = edit_menu::step(&mut seq, &mut ivars.menu.borrow_mut(), event);
        ivars.menu_seq.set(seq);
        for effect in effects {
            match effect {
                Effect::Present { at } => self.present_menu(at),
                Effect::Move => {
                    let interaction = self.ivars().edit_menu.borrow().clone();
                    if let Some(interaction) = interaction {
                        // SAFETY: the interaction is the `UIEditMenuInteraction` `present_menu`
                        // made, and the method takes a `BOOL`. The interaction then asks the
                        // view for the menu's target rect again.
                        let _: () = unsafe { msg_send![&*interaction, updateVisibleMenuPositionAnimated: true] };
                    }
                }
                Effect::Answer(picked) => {
                    self.with_surface(|surface| surface.popup_pick(picked));
                    self.pace(Event::Wake);
                }
                Effect::TimerAfter(attempt, delay) => {
                    let view = MainOnly(Weak::new(self));
                    after_main(delay, move || view.menu_left(attempt));
                }
            }
        }
    }

    /// Presents the edit menu from `at`. The interaction asks the view for its items and its
    /// place.
    fn present_menu(&self, (x, y): (f32, f32)) {
        let interaction = self.ivars().edit_menu.borrow_mut().get_or_insert_with(|| {
            // SAFETY: `initWithDelegate:` is the interaction's initializer, the view conforms
            // to its delegate protocol, and a view keeps the interactions added to it.
            unsafe {
                let interaction: Allocated<AnyObject> = msg_send![class!(UIEditMenuInteraction), alloc];
                let interaction: Retained<AnyObject> = msg_send![interaction, initWithDelegate: self];
                let _: () = msg_send![self, addInteraction: &*interaction];
                interaction
            }
        }).clone();
        // SAFETY: a configuration with no identifier at a point of the view, presented by the
        // view's own interaction.
        unsafe {
            let at = CGPoint::new(x as CGFloat, y as CGFloat);
            let configuration: Retained<AnyObject> = msg_send![class!(UIEditMenuConfiguration), configurationWithIdentifier: None::<&AnyObject>, sourcePoint: at];
            let _: () = msg_send![&*interaction, presentEditMenuWithConfiguration: &*configuration];
        }
    }
}

/// `menu` as a `UIMenu`: an item is a command that calls `runeMenuPick:` with its id, and the
/// items between separators are a group set apart.
fn build(menu: &PopupMenu, title: &str, options: usize) -> Retained<AnyObject> {
    let groups: Vec<&[PopupEntry]> = menu.entries.split(|entry| matches!(entry, PopupEntry::Separator)).filter(|group| !group.is_empty()).collect();
    let children: Vec<Retained<AnyObject>> = match groups.as_slice() {
        [only] => only.iter().map(element).collect(),
        groups => groups.iter().map(|group| ui_menu("", INLINE, group.iter().map(element).collect())).collect(),
    };
    ui_menu(title, options, children)
}

fn element(entry: &PopupEntry) -> Retained<AnyObject> {
    match entry {
        PopupEntry::Item(item) => {
            // SAFETY: a command with a title, no image, a selector the view implements and a
            // property list; its attributes and state are option sets of these sizes.
            unsafe {
                let command: Retained<AnyObject> = msg_send![class!(UICommand), commandWithTitle: &*NSString::from_str(&item.title),
                    image: None::<&AnyObject>, action: sel!(runeMenuPick:), propertyList: &*NSString::from_str(&item.id)];
                if !item.enabled { let _: () = msg_send![&*command, setAttributes: DISABLED]; }
                if item.checked { let _: () = msg_send![&*command, setState: ON]; }
                command
            }
        }
        PopupEntry::Submenu { title, menu, .. } => build(menu, title, 0),
        // `build` splits on separators before it gets here.
        PopupEntry::Separator => ui_menu("", INLINE, Vec::new()),
    }
}

fn ui_menu(title: &str, options: usize, children: Vec<Retained<AnyObject>>) -> Retained<AnyObject> {
    // SAFETY: `addObject:` takes an object, which each element is. The menu's initializer takes
    // an `NSString`, a `UIImage` or nil, an identifier or nil, a `UIMenuOptions` (an
    // `NSUInteger`) and an `NSArray` of menu elements.
    unsafe {
        let list: Retained<AnyObject> = msg_send![class!(NSMutableArray), array];
        for child in &children { let _: () = msg_send![&*list, addObject: &**child]; }
        msg_send![class!(UIMenu), menuWithTitle: &*NSString::from_str(title), image: None::<&AnyObject>,
            identifier: None::<&AnyObject>, options: options, children: &*list]
    }
}
