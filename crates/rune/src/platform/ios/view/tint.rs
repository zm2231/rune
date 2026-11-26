//! A `RuneView`'s tint, which is its app's accent. A view takes its `tintColor` from the views
//! above it unless it has one of its own, which is where a UIKit host's `tintColor` and a
//! SwiftUI host's `.tint()` land, and a window with none set gives the system's blue. The accent is
//! whatever UIKit reports as the view's tint, including its dimmed tint while an alert or
//! sheet is over the view. UIKit says when the tint changes (`tintColorDidChange`). A dynamic
//! colour is resolved for the view's own traits, so one that differs with increased contrast
//! follows that setting when the traits change.

#![allow(unsafe_code)]

use objc2_core_foundation::CGFloat;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};

use super::super::pacing::Event;
use super::super::tint::{self, Components};
use super::RuneView;
use super::lifecycle::{STYLE_DARK, STYLE_LIGHT};
use crate::SystemAccent;

impl RuneView {
    /// The view's tint as an accent: the colour it has in a light and in a dark appearance,
    /// each with the view's other traits.
    pub(super) fn tint(&self) -> SystemAccent {
        // SAFETY: `tintColor` and `traitCollection` are `UIView` properties that hold objects.
        let (color, traits): (Option<Retained<AnyObject>>, Retained<AnyObject>) = unsafe { (msg_send![self, tintColor], msg_send![self, traitCollection]) };
        let [light, dark] = [STYLE_LIGHT, STYLE_DARK].map(|style| color.as_deref().and_then(|color| components(color, &traits, style)));
        tint::accent(light, dark)
    }

    /// The view's tint changed: the app's accent follows, and a frame shows it. A tint that
    /// resolves to the accent the app has draws nothing.
    pub(super) fn tint_changed(&self) {
        let accent = self.tint();
        if self.with_surface(|surface| surface.set_accent(accent)) == Some(true) { self.pace(Event::Wake); }
    }
}

/// `color`'s components as a view with `traits` draws it in the appearance `style`, or none
/// for a colour with no RGB form.
fn components(color: &AnyObject, traits: &AnyObject, style: isize) -> Option<Components> {
    let (mut r, mut g, mut b, mut a): (CGFloat, CGFloat, CGFloat, CGFloat) = (0.0, 0.0, 0.0, 0.0);
    // SAFETY: `traitCollectionWithUserInterfaceStyle:` takes an `NSInteger` enum;
    // `arrayWithObjects:count:` reads `count` objects from the pointer, which holds two that
    // outlive the call; `traitCollectionWithTraitsFromCollections:` takes an array of trait
    // collections, where a later one's traits win; `resolvedColorWithTraitCollection:` takes
    // one and returns a colour; and `getRed:green:blue:alpha:` writes four `CGFloat`s and
    // answers whether it could.
    let converted: bool = unsafe {
        let appearance: Retained<AnyObject> = msg_send![class!(UITraitCollection), traitCollectionWithUserInterfaceStyle: style];
        let both = [traits as *const AnyObject, &*appearance as *const AnyObject];
        let both: Retained<AnyObject> = msg_send![class!(NSArray), arrayWithObjects: both.as_ptr(), count: both.len()];
        let traits: Retained<AnyObject> = msg_send![class!(UITraitCollection), traitCollectionWithTraitsFromCollections: &*both];
        let resolved: Retained<AnyObject> = msg_send![color, resolvedColorWithTraitCollection: &*traits];
        msg_send![&*resolved, getRed: &mut r, green: &mut g, blue: &mut b, alpha: &mut a]
    };
    converted.then_some([r, g, b, a])
}
