//! The focused text field as a platform's own text system sees it (iOS's `UITextInput`, an
//! Android `InputConnection`): its text as drawn, with an input method's composition in place
//! and a secure field masked, the selection and composition as byte ranges of that text, and the
//! field's keyboard hints. The host's text system edits it with [`HostEdit`]s, which land in the
//! app's model at once, and asks where its bytes are drawn. Typed characters, Backspace and
//! Return go through [`Runtime::key`] as a hardware key's would, so key hooks still see them.
//! The host keeps an on-screen keyboard's height in [`Runtime::set_keyboard_inset`].

use std::ops::Range;

use super::*;

mod doc;
mod geometry;
mod keyboard;

/// The focused editable text field, as [`Runtime::host_text`] reads it from the app's model.
/// Every offset is a UTF-8 byte offset into [`Self::text`].
#[derive(Clone, PartialEq)]
#[non_exhaustive]
pub struct HostText {
    /// The field's key path, which [`Runtime::host_text_edit`] and the geometry calls take.
    pub key: String,
    /// The text as the field draws it: a composition in place of the selection it replaces, and
    /// a secure field's bullets instead of its secret.
    pub text: String,
    /// Where the selection starts and where it ends (the caret's side). While composing, the
    /// composition's own cursor or selection.
    pub anchor: usize,
    pub focus: usize,
    /// The composition (marked text), while an input method composes.
    pub marked: Option<Range<usize>>,
    pub secure: bool,
    pub single_line: bool,
    pub hints: crate::TextHints,
    /// The field's drawn box `[x, y, w, h]` in window points, as of the last frame.
    pub rect: [f32; 4],
    /// The part of [`Self::rect`] its scroll viewports and the window show, `[x, y, w, h]`; zero
    /// size when none is.
    pub visible: [f32; 4],
}

impl std::fmt::Debug for HostText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostText")
            .field("key", &self.key)
            .field("len", &self.text.len())
            .field("anchor", &self.anchor)
            .field("focus", &self.focus)
            .field("marked", &self.marked)
            .field("secure", &self.secure)
            .field("single_line", &self.single_line)
            .finish_non_exhaustive()
    }
}

impl HostText {
    /// The selection as an ordered range.
    pub fn selection(&self) -> Range<usize> {
        self.anchor.min(self.focus)..self.anchor.max(self.focus)
    }
}

/// An edit from the platform's text system, in [`HostText::text`]'s byte offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HostEdit {
    /// Writes `text` over `range`, ending any composition: autocorrect, dictation, a predictive
    /// suggestion, a paste, or a composition's final text written over its marked range.
    Replace { range: Range<usize>, text: String },
    /// Moves the selection. Inside the marked text it moves the composition's cursor; anywhere
    /// else the composition is kept as typed text first.
    Select { anchor: usize, focus: usize },
    /// The composition becomes `text`, over the selection, with `cursor` (bytes of `text`) as
    /// its own cursor or selection. Empty text ends it, writing nothing.
    Compose { text: String, cursor: Option<Range<usize>> },
    /// Keeps the composition as typed text.
    FinishComposition,
}

impl<S: 'static> Runtime<S> {
    /// The focused editable text field from the app's model, including edits made since the last
    /// frame. `None` when nothing editable has focus: no field, a read-only or disabled one.
    pub fn host_text(&mut self) -> Option<HostText> {
        let (core, mut app) = self.split_mut();
        core.host_text(&mut app)
    }

    /// Applies `edit` to the field keyed `key` (a [`HostText::key`]) in the app's model now, as
    /// typing does: its change handlers run and the next frame draws it. False when that field
    /// no longer has focus or isn't editable.
    pub fn host_text_edit(&mut self, key: &str, edit: HostEdit) -> bool {
        let (core, mut app) = self.split_mut();
        core.host_text_edit(&mut app, key, edit)
    }

    /// Text the platform's keyboard typed outside a composition: a line break is Return, anything
    /// else types as a hardware key's text does, through the field's key hooks, on the next frame.
    pub fn host_text_type(&mut self, text: &str) {
        self.key(doc::typed_key(text));
    }

    /// The platform's text system let go of the focused field `key` (iOS dismissed its
    /// keyboard): the field blurs on the next frame. Ignored when another field has focus by now.
    pub fn host_text_blur(&mut self, key: &str) -> bool {
        self.core_mut().host_text_blur(key)
    }

    /// The line boxes of `range` of the focused field `key`, `[x, y, w, h]` each in window points
    /// where the last frame drew them. Empty when the range is empty or the field isn't focused.
    pub fn host_text_rects(&self, key: &str, range: Range<usize>) -> Vec<[f32; 4]> {
        self.core().host_text_rects(key, range)
    }

    /// The caret at byte `at` of the focused field `key`, `[x, y, w, h]` in window points where
    /// the last frame drew it.
    pub fn host_text_caret(&self, key: &str, at: usize) -> Option<[f32; 4]> {
        self.core().host_text_caret(key, at)
    }

    /// The byte of the focused field `key` nearest window point (`x`, `y`), as a tap would place
    /// the caret there.
    pub fn host_text_offset_at(&self, key: &str, x: f32, y: f32) -> Option<usize> {
        self.core().host_text_offset_at(key, x, y)
    }

    /// The height in points an on-screen keyboard covers at the bottom of the window, as
    /// [`Cx::keyboard_inset`] reports it; 0 when none does. A focused field scrolls above it, and
    /// a change re-runs the view. Negative and non-finite values count as 0.
    pub fn set_keyboard_inset(&mut self, bottom: f32) {
        self.core_mut().set_keyboard_inset(bottom);
    }

    /// Whether the host's text system owns a long press in the focused editable field (iOS's
    /// loupe and edit menu), so it opens no field menu of Rune's. Off by default.
    pub fn set_native_text_selection(&mut self, on: bool) {
        self.core_mut().edit.native_text_selection = on;
    }
}

impl RuntimeCore {
    /// Whether the focused field takes edits from a host: it has a model and isn't read-only or
    /// disabled.
    fn host_editable(&self) -> bool {
        !self.focused_input_read_only()
            && !self.focused_input_disabled()
            && self.focused_input().is_some_and(|i| i.access.is_some())
    }

    pub(crate) fn host_text(&mut self, app: &mut dyn AppDriver) -> Option<HostText> {
        if !self.host_editable() { return None; }
        let key = self.kbd.focus.path()?.to_owned();
        let input = self.focused_input()?;
        let (single_line, hint_spec, access) = (input.single_line, input.hints, input.access.clone()?);
        let edit = access(app.states());
        let shape = doc::shape(edit);
        let secure = edit.is_secure();
        let (rect, visible) = self.host_text_frame(&key).unwrap_or_default();
        Some(HostText {
            key,
            text: shape.text,
            anchor: shape.anchor,
            focus: shape.focus,
            marked: shape.marked,
            secure,
            single_line,
            hints: hint_spec.resolve(secure),
            rect,
            visible,
        })
    }

    pub(crate) fn host_text_edit(&mut self, app: &mut dyn AppDriver, key: &str, edit: HostEdit) -> bool {
        if self.kbd.focus.path() != Some(key) || !self.host_editable() { return false; }
        let mut tx = self.transaction(app);
        let ran = self.with_focused(app, |e| {
            doc::apply(e, edit);
            EditOutcome::default()
        }, &mut tx);
        if ran { self.commit.dirty = true; }
        merge(&mut self.frame.pending, tx);
        ran
    }

    /// A long press at (`x`, `y`) is the host text system's: it lands in the focused editable
    /// field while the host owns text selection.
    pub(in crate::runtime) fn host_owns_hold(&self, x: f32, y: f32) -> bool {
        if !self.edit.native_text_selection || !self.host_editable() { return false; }
        let Some(key) = self.kbd.focus.path() else { return false };
        self.host_text_frame(key).is_some_and(|(_, [vx, vy, w, h])| x >= vx && x <= vx + w && y >= vy && y <= vy + h)
    }

    pub(crate) fn host_text_blur(&mut self, key: &str) -> bool {
        if key.is_empty() || self.kbd.focus.path() != Some(key) || self.focused_input().is_none() { return false; }
        self.frame.queue.push(Action::Blur);
        true
    }
}

#[cfg(test)]
#[path = "host_text_tests.rs"]
mod tests;
