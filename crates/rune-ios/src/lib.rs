//! Runs a Rune app on iPhone. [`run`] starts UIKit with a scene that fills its window with a
//! [`RuneView`]: a `UIView` on a `CAMetalLayer` that drives `rune::embed::LayerHost`.
//!
//! The view ticks a `CADisplayLink` at up to 120 Hz while something moves and pauses it once a
//! frame settles. Touches, the safe area, dark mode, Reduce Motion and haptics go to and from
//! the runtime, and the link stops while the app is in the background, where Metal may not
//! draw. It needs iOS 17.5, for haptics attached to the view.
//!
//! The app's `Info.plist` needs `UIApplicationSceneManifest` (one scene, any configuration: the
//! app delegate names the scene delegate) and `CADisableMinimumFrameDurationOnPhone` set to
//! true for 120 Hz on ProMotion iPhones; `scripts/ios-app.sh` writes both.
//!
//! Off iOS the crate builds empty, so a workspace check doesn't need the iOS SDK.

#[cfg(target_os = "ios")]
mod ios;
#[cfg_attr(not(any(test, target_os = "ios")), allow(dead_code))]
mod pacing;

#[cfg(target_os = "ios")]
pub use ios::RuneView;
#[cfg(target_os = "ios")]
pub use ios::run;
