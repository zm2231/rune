//! `RuneTextInput`: what the iOS keyboard talks to. A hidden view inside a `RuneView` that
//! becomes first responder while a text field has the focus, and answers UIKit's text input
//! protocol from the keyboard's copy of the field (`doc`): the soft keyboard, a hardware
//! one, autocorrection, dictation and marked text all edit through it.
//!
//! Positions and ranges are UTF-16 offsets into that copy. Rune draws its own caret and
//! selection; UIKit draws the handles of a selection and the loupe while one is dragged. A
//! position's caret rect, a range's first rect and the position nearest a point come from the
//! field's lines as last drawn (`geometry`), which place the keyboard's own bubbles and move
//! the space bar's cursor. While the keyboard composes or the field is a password, the copy
//! is not the field's text, and every position has the caret's rect.
//!
//! The keyboard reads the field's traits (which keys, capitals, correction, the Return key's
//! caption, what the system may offer) when it attaches, so it is reloaded when they change
//! under it, once any marked text is closed (`traits::Reload`).

#![allow(unsafe_code)]

use std::cell::{Cell, RefCell};

use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_foundation::{NSArray, NSDictionary, NSRange, NSString};
use objc2::rc::{Allocated, Retained, Weak};
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, class, define_class, extern_class, extern_protocol, msg_send};

use super::doc::{Document, Effect, Event, Snapshot};
use super::geometry::Geometry;
use super::handles;
use super::selection::{self, Display, cg_rect};
use super::traits::{self, ContentType, Frame, Reload, ReloadEffect};
use super::undo::uikit::Undo;
use super::undo::{Action, Offer};

mod grab;
use crate::platform::objc_boundary::guarded;
use crate::machine::Effects;
use crate::input::traits::InputTraits;
use super::super::view::{RuneView, UIResponder, UIView};

extern_class!(
    // SAFETY: UIKit's class of this name is an `NSObject` subclass that UIKit keeps to the main
    // thread.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "UITextPosition"]
    struct UITextPosition;
);

extern_class!(
    // SAFETY: as for `UITextPosition`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "UITextRange"]
    struct UITextRange;
);

extern_protocol!(
    /// Declared without its methods: `RuneTextInput`'s `unsafe impl` block registers them by
    /// selector, and nothing in Rust calls them.
    #[name = "UITextInput"]
    unsafe trait TextInput: NSObjectProtocol {}
);

// SAFETY: UIKit's headers declare each as a `UITextContentType`, an `NSString` that lives for
// the process.
#[link(name = "UIKit", kind = "framework")]
unsafe extern "C" {
    static UITextContentTypeUsername: &'static NSString;
    static UITextContentTypePassword: &'static NSString;
    static UITextContentTypeNewPassword: &'static NSString;
    static UITextContentTypeOneTimeCode: &'static NSString;
    static UITextContentTypeEmailAddress: &'static NSString;
    static UITextContentTypeTelephoneNumber: &'static NSString;
    static UITextContentTypeURL: &'static NSString;
    static UITextContentTypeName: &'static NSString;
    static UITextContentTypeGivenName: &'static NSString;
    static UITextContentTypeFamilyName: &'static NSString;
    static UITextContentTypeOrganizationName: &'static NSString;
    static UITextContentTypeFullStreetAddress: &'static NSString;
    static UITextContentTypeAddressCity: &'static NSString;
    static UITextContentTypeAddressState: &'static NSString;
    static UITextContentTypePostalCode: &'static NSString;
    static UITextContentTypeCountryName: &'static NSString;
}

/// UIKit's constant for `content`.
fn content_type(content: ContentType) -> &'static NSString {
    // SAFETY: each is an immutable static that UIKit sets before any of its views exists.
    unsafe {
        match content {
            ContentType::Username => UITextContentTypeUsername,
            ContentType::Password => UITextContentTypePassword,
            ContentType::NewPassword => UITextContentTypeNewPassword,
            ContentType::OneTimeCode => UITextContentTypeOneTimeCode,
            ContentType::EmailAddress => UITextContentTypeEmailAddress,
            ContentType::TelephoneNumber => UITextContentTypeTelephoneNumber,
            ContentType::URL => UITextContentTypeURL,
            ContentType::Name => UITextContentTypeName,
            ContentType::GivenName => UITextContentTypeGivenName,
            ContentType::FamilyName => UITextContentTypeFamilyName,
            ContentType::OrganizationName => UITextContentTypeOrganizationName,
            ContentType::FullStreetAddress => UITextContentTypeFullStreetAddress,
            ContentType::AddressCity => UITextContentTypeAddressCity,
            ContentType::AddressState => UITextContentTypeAddressState,
            ContentType::PostalCode => UITextContentTypePostalCode,
            ContentType::CountryName => UITextContentTypeCountryName,
        }
    }
}

/// `NSOrderedAscending`, `NSOrderedSame` and `NSOrderedDescending`.
const ORDER: [isize; 3] = [-1, 0, 1];
/// `UITextLayoutDirectionLeft` and `UITextLayoutDirectionUp`: the two that move backward.
const DIRECTION_LEFT: isize = 3;
const DIRECTION_UP: isize = 4;
const DIRECTION_DOWN: isize = 5;
/// `NSWritingDirectionNatural`.
const WRITING_NATURAL: isize = -1;

define_class!(
    // SAFETY: `UITextPosition` is made to be subclassed, and `Position` has no `Drop`.
    #[unsafe(super(UITextPosition, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RuneTextPosition"]
    #[ivars = usize]
    struct Position;
);

impl Position {
    /// UTF-16 offset `at` of the document.
    fn new(mtm: MainThreadMarker, at: usize) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(at);
        // SAFETY: `init` is `UITextPosition`'s initializer.
        unsafe { msg_send![super(this), init] }
    }

    fn at(&self) -> usize { *self.ivars() }
}

define_class!(
    // SAFETY: `UITextRange` is made to be subclassed with these three overridden, and `Span`
    // has no `Drop`.
    #[unsafe(super(UITextRange, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RuneTextRange"]
    #[ivars = (usize, usize)]
    struct Span;

    impl Span {
        #[unsafe(method_id(start))]
        fn start(&self) -> Retained<Position> {
            guarded("start", || Position::new(self.mtm(), self.ivars().0))
        }

        #[unsafe(method_id(end))]
        fn end(&self) -> Retained<Position> {
            guarded("end", || Position::new(self.mtm(), self.ivars().1))
        }

        #[unsafe(method(isEmpty))]
        fn is_empty(&self) -> bool {
            guarded("isEmpty", || self.ivars().0 == self.ivars().1)
        }
    }
);

impl Span {
    /// UTF-16 range `range` of the document.
    fn new(mtm: MainThreadMarker, range: std::ops::Range<usize>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars((range.start, range.end.max(range.start)));
        // SAFETY: `init` is `UITextRange`'s initializer.
        unsafe { msg_send![super(this), init] }
    }

    fn range(&self) -> std::ops::Range<usize> { self.ivars().0..self.ivars().1 }
}

pub(crate) struct Ivars {
    /// The `RuneView` this sits in, which carries the keyboard's edits to the runtime.
    host: Weak<RuneView>,
    doc: RefCell<Document>,
    /// The caret's rect in the host view's points.
    caret: Cell<CGRect>,
    /// The focused field is a password.
    secure: Cell<bool>,
    /// What the focused field asks of the keyboard.
    traits: Cell<InputTraits>,
    /// Whether the attached keyboard has read `secure` and `traits`.
    reload: Cell<Reload>,
    /// UIKit's `inputDelegate`, which hears that the text or selection changed under it.
    delegate: RefCell<Option<Weak<AnyObject>>>,
    tokenizer: RefCell<Option<Retained<AnyObject>>>,
    /// The cursor that follows the finger while the space bar is a trackpad, once made.
    floating: RefCell<Option<Retained<AnyObject>>>,
    /// What the system's undo gestures ask: an undo manager that offers what the field can do.
    undo: Undo,
    display: RefCell<Display>,
    handles: RefCell<handles::State>,
}

define_class!(
    // SAFETY: `UIView` is made to be subclassed, and `RuneTextInput` has no `Drop`.
    #[unsafe(super(UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RuneTextInput"]
    #[ivars = Ivars]
    pub(crate) struct RuneTextInput;

    // SAFETY: the superclass chain ends in `NSObject`, which implements the protocol.
    unsafe impl NSObjectProtocol for RuneTextInput {}

    impl RuneTextInput {
        #[unsafe(method(canBecomeFirstResponder))]
        fn can_become_first_responder(&self) -> bool {
            guarded("canBecomeFirstResponder", || true)
        }

        // The standard edit actions, which UIKit sends a first responder for the three-finger
        // copy, cut and paste gestures and from any system menu. The field does each itself,
        // as for the hardware keyboard's shortcut, with Rune's clipboard.
        #[unsafe(method(selectAll:))]
        fn select_all(&self, _sender: Option<&AnyObject>) { guarded("selectAll:", || { self.command('a'); }) }

        #[unsafe(method(copy:))]
        fn copy(&self, _sender: Option<&AnyObject>) { guarded("copy:", || { self.command('c'); }) }

        #[unsafe(method(cut:))]
        fn cut(&self, _sender: Option<&AnyObject>) { guarded("cut:", || { self.command('x'); }) }

        #[unsafe(method(paste:))]
        fn paste(&self, _sender: Option<&AnyObject>) { guarded("paste:", || { self.command('v'); }) }

        /// What a shake and the three-finger undo gestures ask whether there is anything to
        /// undo or redo, and tell to do it: the focused field's history, through a manager
        /// that holds one action for each. Nil with no field focused, so nothing is offered.
        #[unsafe(method_id(undoManager))]
        fn undo_manager(&self) -> Option<Retained<AnyObject>> {
            guarded("undoManager", || self.ivars().undo.manager())
        }

        #[unsafe(method(runeUndo:))]
        fn undo_fired(&self, _object: Option<&AnyObject>) { guarded("runeUndo:", || { self.history(Action::Undo); }) }

        #[unsafe(method(runeRedo:))]
        fn redo_fired(&self, _object: Option<&AnyObject>) { guarded("runeRedo:", || { self.history(Action::Redo); }) }

        /// A hardware key going down. The host hears it as a physical key, and takes it from
        /// UIKit when it names an edit, a move or a shortcut. UIKit types the key when it is a
        /// character, and has it to itself while the keyboard is composing.
        #[unsafe(method(pressesBegan:withEvent:))]
        fn presses_began(&self, presses: &AnyObject, event: Option<&AnyObject>) {
            guarded("pressesBegan:withEvent:", || {
                let composing = self.ivars().doc.borrow().marked().is_some();
                let taken = !composing && self.ivars().host.load().is_some_and(|host| host.presses(presses, true, true));
                if !taken {
                    // SAFETY: `presses` and `event` are UIKit's own objects, passed unchanged to
                    // `UIView`'s method of the same signature.
                    let _: () = unsafe { msg_send![super(self), pressesBegan: presses, withEvent: event] };
                }
            })
        }

        #[unsafe(method(pressesEnded:withEvent:))]
        fn presses_ended(&self, presses: &AnyObject, event: Option<&AnyObject>) {
            guarded("pressesEnded:withEvent:", || {
                if let Some(host) = self.ivars().host.load() { host.presses(presses, false, true); }
                // SAFETY: as in `pressesBegan:withEvent:`.
                let _: () = unsafe { msg_send![super(self), pressesEnded: presses, withEvent: event] };
            })
        }

        #[unsafe(method(pressesCancelled:withEvent:))]
        fn presses_cancelled(&self, presses: &AnyObject, event: Option<&AnyObject>) {
            guarded("pressesCancelled:withEvent:", || {
                if let Some(host) = self.ivars().host.load() { host.presses(presses, false, true); }
                // SAFETY: as in `pressesBegan:withEvent:`.
                let _: () = unsafe { msg_send![super(self), pressesCancelled: presses, withEvent: event] };
            })
        }

        #[unsafe(method(isSecureTextEntry))]
        fn is_secure(&self) -> bool {
            guarded("isSecureTextEntry", || self.ivars().secure.get())
        }

        #[unsafe(method(keyboardType))]
        fn keyboard_type(&self) -> isize {
            guarded("keyboardType", || traits::keyboard_type(self.ivars().traits.get().mode))
        }

        #[unsafe(method(textContentType))]
        fn text_content_type(&self) -> Option<&'static NSString> {
            guarded("textContentType", || traits::content_type(self.ivars().traits.get().autocomplete).map(content_type))
        }

        #[unsafe(method(autocapitalizationType))]
        fn autocapitalization_type(&self) -> isize {
            guarded("autocapitalizationType", || traits::autocapitalization_type(self.ivars().traits.get().autocapitalize))
        }

        #[unsafe(method(autocorrectionType))]
        fn autocorrection_type(&self) -> isize {
            guarded("autocorrectionType", || traits::correction_type(self.ivars().traits.get().autocorrect))
        }

        #[unsafe(method(spellCheckingType))]
        fn spell_checking_type(&self) -> isize {
            guarded("spellCheckingType", || traits::correction_type(self.ivars().traits.get().spellcheck))
        }

        #[unsafe(method(returnKeyType))]
        fn return_key_type(&self) -> isize {
            guarded("returnKeyType", || traits::return_key_type(self.ivars().traits.get().enter_key_hint))
        }

        #[unsafe(method(hasText))]
        fn has_text(&self) -> bool {
            guarded("hasText", || self.ivars().doc.borrow().len() > 0)
        }

        #[unsafe(method(insertText:))]
        fn insert_text(&self, text: &NSString) {
            guarded("insertText:", || { self.edit(Event::Insert(text.to_string())); })
        }

        #[unsafe(method(deleteBackward))]
        fn delete_backward(&self) {
            guarded("deleteBackward", || { self.edit(Event::DeleteBackward); })
        }
    }

    // SAFETY: this block implements every required method of `UITextInput`, each with the
    // signature `UITextInput.h` declares, and the floating cursor's three optional ones. The
    // required methods of `UIKeyInput`, which the protocol inherits, are in the block above.
    unsafe impl TextInput for RuneTextInput {
        #[unsafe(method_id(textInRange:))]
        fn text_in_range(&self, range: &Span) -> Option<Retained<NSString>> {
            guarded("textInRange:", || Some(NSString::from_str(&self.ivars().doc.borrow().slice(range.range()))))
        }

        #[unsafe(method(replaceRange:withText:))]
        fn replace_range(&self, range: &Span, text: &NSString) {
            guarded("replaceRange:withText:", || { self.edit(Event::Replace { range: range.range(), text: text.to_string() }); })
        }

        #[unsafe(method_id(selectedTextRange))]
        fn selected_text_range(&self) -> Option<Retained<Span>> {
            guarded("selectedTextRange", || Some(Span::new(self.mtm(), self.ivars().doc.borrow().selection())))
        }

        #[unsafe(method(setSelectedTextRange:))]
        fn set_selected_text_range(&self, range: Option<&Span>) {
            guarded("setSelectedTextRange:", || if let Some(range) = range { self.edit(Event::Select(range.range())); })
        }

        #[unsafe(method_id(markedTextRange))]
        fn marked_text_range(&self) -> Option<Retained<Span>> {
            guarded("markedTextRange", || self.ivars().doc.borrow().marked().map(|range| Span::new(self.mtm(), range)))
        }

        #[unsafe(method_id(markedTextStyle))]
        fn marked_text_style(&self) -> Option<Retained<NSDictionary>> {
            guarded("markedTextStyle", || None)
        }

        /// Rune draws the composition its own way.
        #[unsafe(method(setMarkedTextStyle:))]
        fn set_marked_text_style(&self, _style: Option<&NSDictionary>) { guarded("setMarkedTextStyle:", || {}) }

        #[unsafe(method(setMarkedText:selectedRange:))]
        fn set_marked_text(&self, text: Option<&NSString>, selected: NSRange) {
            guarded("setMarkedText:selectedRange:", || {
                let text = text.map(NSString::to_string).unwrap_or_default();
                self.edit(Event::SetMarked { text, selection: selected.location..selected.location + selected.length });
            })
        }

        #[unsafe(method(unmarkText))]
        fn unmark_text(&self) {
            guarded("unmarkText", || { self.edit(Event::Unmark); })
        }

        #[unsafe(method_id(beginningOfDocument))]
        fn beginning_of_document(&self) -> Retained<Position> {
            guarded("beginningOfDocument", || Position::new(self.mtm(), 0))
        }

        #[unsafe(method_id(endOfDocument))]
        fn end_of_document(&self) -> Retained<Position> {
            guarded("endOfDocument", || Position::new(self.mtm(), self.ivars().doc.borrow().len()))
        }

        #[unsafe(method_id(textRangeFromPosition:toPosition:))]
        fn text_range(&self, from: &Position, to: &Position) -> Option<Retained<Span>> {
            guarded("textRangeFromPosition:toPosition:", || Some(Span::new(self.mtm(), from.at().min(to.at())..from.at().max(to.at()))))
        }

        #[unsafe(method_id(positionFromPosition:offset:))]
        fn position_offset(&self, position: &Position, offset: isize) -> Option<Retained<Position>> {
            guarded("positionFromPosition:offset:", || self.moved(position.at(), offset))
        }

        #[unsafe(method_id(positionFromPosition:inDirection:offset:))]
        fn position_in_direction(&self, position: &Position, direction: isize, offset: isize) -> Option<Retained<Position>> {
            guarded("positionFromPosition:inDirection:offset:", || {
                let signed = if backward(direction) { -offset } else { offset };
                // A line up or down where the field's lines are known, else by characters.
                let line = (direction == DIRECTION_UP || direction == DIRECTION_DOWN)
                    .then(|| self.place(position.at())).flatten()
                    .and_then(|(geometry, byte)| geometry.vertical(byte, signed));
                match line {
                    Some(byte) => Some(Position::new(self.mtm(), self.ivars().doc.borrow().offset_of(byte))),
                    None => self.moved(position.at(), signed),
                }
            })
        }

        #[unsafe(method(comparePosition:toPosition:))]
        fn compare(&self, a: &Position, b: &Position) -> isize {
            guarded("comparePosition:toPosition:", || ORDER[(a.at().cmp(&b.at()) as isize + 1) as usize])
        }

        #[unsafe(method(offsetFromPosition:toPosition:))]
        fn offset(&self, from: &Position, to: &Position) -> isize {
            guarded("offsetFromPosition:toPosition:", || to.at() as isize - from.at() as isize)
        }

        #[unsafe(method_id(inputDelegate))]
        fn input_delegate(&self) -> Option<Retained<AnyObject>> {
            guarded("inputDelegate", || self.delegate())
        }

        #[unsafe(method(setInputDelegate:))]
        fn set_input_delegate(&self, delegate: Option<&AnyObject>) {
            guarded("setInputDelegate:", || { *self.ivars().delegate.borrow_mut() = delegate.map(Weak::new); })
        }

        #[unsafe(method_id(tokenizer))]
        fn tokenizer(&self) -> Retained<AnyObject> {
            guarded("tokenizer", || {
                self.ivars().tokenizer.borrow_mut().get_or_insert_with(|| {
                    // SAFETY: UIKit's own tokenizer, made for a view that conforms to `UITextInput`.
                    unsafe {
                        let tokenizer: Allocated<AnyObject> = msg_send![class!(UITextInputStringTokenizer), alloc];
                        msg_send![tokenizer, initWithTextInput: self]
                    }
                }).clone()
            })
        }

        #[unsafe(method_id(positionWithinRange:farthestInDirection:))]
        fn farthest(&self, range: &Span, direction: isize) -> Option<Retained<Position>> {
            guarded("positionWithinRange:farthestInDirection:", || Some(Position::new(self.mtm(), if backward(direction) { range.range().start } else { range.range().end })))
        }

        #[unsafe(method_id(characterRangeByExtendingPosition:inDirection:))]
        fn extending(&self, position: &Position, direction: isize) -> Option<Retained<Span>> {
            guarded("characterRangeByExtendingPosition:inDirection:", || {
                let end = if backward(direction) { 0 } else { self.ivars().doc.borrow().len() };
                Some(Span::new(self.mtm(), position.at().min(end)..position.at().max(end)))
            })
        }

        #[unsafe(method(baseWritingDirectionForPosition:inDirection:))]
        fn base_writing_direction(&self, _position: &Position, _direction: isize) -> isize {
            guarded("baseWritingDirectionForPosition:inDirection:", || WRITING_NATURAL)
        }

        /// The field takes its direction from its text.
        #[unsafe(method(setBaseWritingDirection:forRange:))]
        fn set_base_writing_direction(&self, _direction: isize, _range: &Span) { guarded("setBaseWritingDirection:forRange:", || {}) }

        #[unsafe(method(firstRectForRange:))]
        fn first_rect(&self, range: &Span) -> CGRect {
            guarded("firstRectForRange:", || {
                let range = range.range();
                let rect = self.place(range.start).zip(self.ivars().doc.borrow().field_byte(range.end))
                    .and_then(|((geometry, from), to)| geometry.first_rect(from..to));
                rect.map_or(self.ivars().caret.get(), cg_rect)
            })
        }

        #[unsafe(method(caretRectForPosition:))]
        fn caret_rect(&self, position: &Position) -> CGRect {
            guarded("caretRectForPosition:", || {
                let rect = self.place(position.at()).and_then(|(geometry, byte)| geometry.caret(byte));
                rect.map_or(self.ivars().caret.get(), cg_rect)
            })
        }

        #[unsafe(method_id(selectionRectsForRange:))]
        fn selection_rects(&self, range: &Span) -> Retained<NSArray> {
            guarded("selectionRectsForRange:", || {
                let range = range.range();
                let rects = self.place(range.start).zip(self.ivars().doc.borrow().field_byte(range.end))
                    .map(|((geometry, from), to)| geometry.selection_rects(from..to)).unwrap_or_default();
                selection::rects(self.mtm(), &rects)
            })
        }

        #[unsafe(method_id(closestPositionToPoint:))]
        fn closest_position(&self, point: CGPoint) -> Option<Retained<Position>> {
            guarded("closestPositionToPoint:", || Some(Position::new(self.mtm(), self.closest(point))))
        }

        #[unsafe(method_id(closestPositionToPoint:withinRange:))]
        fn closest_position_within(&self, point: CGPoint, range: &Span) -> Option<Retained<Position>> {
            guarded("closestPositionToPoint:withinRange:", || Some(Position::new(self.mtm(), self.closest(point).clamp(range.range().start, range.range().end))))
        }

        #[unsafe(method_id(characterRangeAtPoint:))]
        fn character_range_at(&self, _point: CGPoint) -> Option<Retained<Span>> {
            guarded("characterRangeAtPoint:", || None)
        }

        // The space bar as a trackpad. UIKit moves the selection through
        // `closestPositionToPoint:` as the finger moves, and asks the view to draw the cursor
        // that floats under the finger between those positions.
        #[unsafe(method(beginFloatingCursorAtPoint:))]
        fn begin_floating_cursor(&self, point: CGPoint) {
            guarded("beginFloatingCursorAtPoint:", || { self.float(Some(point), false); })
        }

        #[unsafe(method(updateFloatingCursorAtPoint:))]
        fn update_floating_cursor(&self, point: CGPoint) {
            guarded("updateFloatingCursorAtPoint:", || { self.float(Some(point), false); })
        }

        #[unsafe(method(endFloatingCursor))]
        fn end_floating_cursor(&self) {
            guarded("endFloatingCursor", || { self.float(None, true); })
        }
    }
);

/// Whether a `UITextLayoutDirection` moves toward the start of the text.
fn backward(direction: isize) -> bool {
    direction == DIRECTION_LEFT || direction == DIRECTION_UP
}

impl RuneTextInput {
    /// A hidden, empty view for `host`, which adds it to itself.
    pub(crate) fn new(mtm: MainThreadMarker, host: &RuneView) -> Retained<Self> {
        let zero = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(0.0, 0.0));
        let this = Self::alloc(mtm).set_ivars(Ivars {
            host: Weak::new(host),
            doc: RefCell::new(Document::default()),
            caret: Cell::new(zero),
            secure: Cell::new(false),
            traits: Cell::new(InputTraits::default()),
            reload: Cell::new(Reload::default()),
            delegate: RefCell::new(None),
            tokenizer: RefCell::new(None),
            floating: RefCell::new(None),
            undo: Undo::new(),
            display: RefCell::default(),
            handles: RefCell::default(),
        });
        // SAFETY: `initWithFrame:` is `UIView`'s designated initializer.
        unsafe { msg_send![super(this), initWithFrame: zero] }
    }

    /// The field's geometry and the byte of its text at UTF-16 offset `at` of the document,
    /// where the document is the field's text and the field has drawn.
    fn place(&self, at: usize) -> Option<(Geometry, usize)> {
        let byte = self.ivars().doc.borrow().field_byte(at)?;
        Some((self.ivars().host.load()?.text_geometry()?, byte))
    }

    /// The document offset nearest `point`: by the field's lines, or the caret's where they
    /// are not known.
    fn closest(&self, point: CGPoint) -> usize {
        let doc = self.ivars().doc.borrow();
        let caret = doc.selection().end;
        let found = self.place(caret).and_then(|(geometry, _)| geometry.closest(point.x as f32, point.y as f32));
        found.map_or(caret, |byte| doc.offset_of(byte))
    }

    /// Moves the floating cursor to `point`, or with none, settles it onto the caret and fades
    /// it out. It is a thin bar in the view's tint, the caret's height and a little more, with
    /// a soft shadow that lifts it off the text, as UIKit's own fields draw theirs.
    fn float(&self, point: Option<CGPoint>, animated: bool) {
        /// How much wider and taller than the caret the floating cursor is, in points.
        const WIDTH: CGFloat = 3.0;
        const GROW: CGFloat = 4.0;
        /// Seconds the cursor takes to settle onto the caret.
        const SETTLE: f64 = 0.2;
        let caret = self.ivars().caret.get();
        let cursor = self.ivars().floating.borrow_mut().get_or_insert_with(|| {
            // SAFETY: a plain view added to this one; its layer's corner radius and shadow
            // are `CGFloat`s, a `float` and a `CGSize`.
            unsafe {
                let cursor: Retained<AnyObject> = msg_send![class!(UIView), new];
                let tint: Retained<AnyObject> = msg_send![self, tintColor];
                let _: () = msg_send![&*cursor, setBackgroundColor: &*tint];
                let _: () = msg_send![&*cursor, setUserInteractionEnabled: false];
                let layer: Retained<AnyObject> = msg_send![&*cursor, layer];
                let _: () = msg_send![&*layer, setCornerRadius: WIDTH / 2.0];
                let _: () = msg_send![&*layer, setShadowOpacity: 0.3f32];
                let _: () = msg_send![&*layer, setShadowRadius: 3.0 as CGFloat];
                let _: () = msg_send![&*layer, setShadowOffset: CGSize::new(0.0, 2.0)];
                let _: () = msg_send![self, addSubview: &*cursor];
                cursor
            }
        }).clone();
        let height = caret.size.height + GROW;
        let (centre, alpha): (CGPoint, CGFloat) = match point {
            Some(point) => (point, 1.0),
            None => (CGPoint::new(caret.origin.x + caret.size.width / 2.0, caret.origin.y + caret.size.height / 2.0), 0.0),
        };
        let frame = CGRect::new(CGPoint::new(centre.x - WIDTH / 2.0, centre.y - height / 2.0), CGSize::new(WIDTH, height));
        // SAFETY: `UIView`'s animation class methods and a view's frame and alpha. A move
        // under the finger is not animated; the settle onto the caret is.
        unsafe {
            if animated {
                let _: () = msg_send![class!(UIView), beginAnimations: None::<&NSString>, context: std::ptr::null_mut::<std::ffi::c_void>()];
                let _: () = msg_send![class!(UIView), setAnimationDuration: SETTLE];
            }
            let _: () = msg_send![&*cursor, setFrame: frame];
            let _: () = msg_send![&*cursor, setAlpha: alpha];
            if animated { let _: () = msg_send![class!(UIView), commitAnimations]; }
        }
    }

    /// The keyboard's side of the conversation, while it lives.
    fn delegate(&self) -> Option<Retained<AnyObject>> {
        self.ivars().delegate.borrow().as_ref().and_then(Weak::load)
    }

    /// The position `offset` UTF-16 units from `at`, or none past either end of the document.
    fn moved(&self, at: usize, offset: isize) -> Option<Retained<Position>> {
        let to = at as isize + offset;
        (0..=self.ivars().doc.borrow().len() as isize).contains(&to).then(|| Position::new(self.mtm(), to as usize))
    }

    /// Sends the field the shortcut Command plus `c`.
    fn command(&self, c: char) {
        if let Some(host) = self.ivars().host.load() { host.command(c); }
    }

    /// The undo manager ran one of its actions: for a gesture, the field undoes or redoes a
    /// step of its own history, as for ⌘Z and ⇧⌘Z.
    fn history(&self, action: Action) {
        let key = self.ivars().undo.fired(self, action);
        if let (Some(key), Some(host)) = (key, self.ivars().host.load()) { host.history(key == Action::Redo); }
    }

    /// An edit the keyboard made: the copy takes it now, and the runtime hears of it.
    fn edit(&self, event: Event) {
        let effects = self.ivars().doc.borrow_mut().step(event);
        self.tell(effects.clone());
        let Some(host) = self.ivars().host.load() else { return };
        if effects.clone().into_iter().any(|effect| effect.edits_text()) { host.hide_menu(); }
        host.text_edits(effects);
    }

    fn tell(&self, effects: Effects<Effect>) {
        let Some(delegate) = self.delegate() else { return };
        for effect in effects {
            // SAFETY: `UITextInputDelegate`'s methods, each taking the text input.
            unsafe {
                match effect {
                    Effect::TextChanged => {
                        let _: () = msg_send![&*delegate, textWillChange: self];
                        let _: () = msg_send![&*delegate, textDidChange: self];
                    }
                    // UIKit answers news of a selected range by collapsing it to its end (it
                    // draws no selection here to keep), so it hears only of a caret that moved
                    // and reads a range when it next needs one.
                    Effect::SelectionChanged if self.ivars().doc.borrow().selection().is_empty() => {
                        let _: () = msg_send![&*delegate, selectionWillChange: self];
                        let _: () = msg_send![&*delegate, selectionDidChange: self];
                    }
                    Effect::SelectionChanged => {}
                    Effect::Key(_) | Effect::Commit(_) | Effect::Preedit(..) | Effect::Select(_) => {}
                }
            }
        }
    }

    /// Whether the keyboard edits a field through this input now, and the field is not a
    /// password.
    pub(crate) fn readable(&self) -> bool {
        // SAFETY: a `UIResponder` method of this signature.
        !self.ivars().secure.get() && unsafe { msg_send![self, isFirstResponder] }
    }

    /// After a frame: whether a field wants the keyboard, what it holds, where its caret is in
    /// the host's points, whether it is a password, what it asks of the keyboard and what it
    /// can undo and redo. Takes or gives up the keyboard, and tells it when the runtime's text
    /// is not what it believed.
    pub(crate) fn sync(&self, wanted: bool, snapshot: Snapshot, caret: Option<[f32; 4]>, secure: bool, traits: InputTraits, history: Option<Offer>) {
        let ivars = self.ivars();
        ivars.undo.frame(self, history);
        if let Some([x, y, w, h]) = caret {
            ivars.caret.set(CGRect::new(CGPoint::new(x as CGFloat, y as CGFloat), CGSize::new(w as CGFloat, h as CGFloat)));
        }
        ivars.secure.set(secure);
        ivars.traits.set(traits);
        let effects = if wanted { ivars.doc.borrow_mut().step(Event::Runtime(snapshot)) } else { Default::default() };
        self.tell(effects);
        // SAFETY: `UIResponder` methods of these signatures.
        unsafe {
            let first: bool = msg_send![self, isFirstResponder];
            // A keyboard that attaches now reads the traits as it does; one that stays may
            // need to read them again.
            let mut reload = ivars.reload.get();
            let composing = ivars.doc.borrow().marked().is_some();
            let effects = reload.step(Frame { asked: (traits, secure), attached: wanted && first, composing });
            ivars.reload.set(reload);
            if wanted && !first {
                let _: bool = msg_send![self, becomeFirstResponder];
            } else if !wanted && first {
                // The host takes the hardware keyboard back, which also puts the soft one away.
                match ivars.host.load() {
                    Some(host) => { let _: bool = msg_send![&*host, becomeFirstResponder]; }
                    None => { let _: bool = msg_send![self, resignFirstResponder]; }
                }
            } else {
                for ReloadEffect::Reload in effects {
                    let _: () = msg_send![self, reloadInputViews];
                }
            }
        }
        self.show_handles(wanted && !secure);
        self.handles(handles::Event::Frame, None);
    }
}
