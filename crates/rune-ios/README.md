# rune-ios

Runs a Rune app on iPhone.

```rust
fn main() {
    rune_ios::run(rune::AppSpec::new("Hello", (390.0, 844.0), State::default, view));
}
```

`run` starts UIKit with the scene lifecycle and fills the scene's window with a `RuneView`: a
`UIView` whose layer is a `CAMetalLayer`, driving `rune::embed::LayerHost`. An app with its own
UIKit hierarchy can place a `RuneView::new(mtm, spec)` anywhere instead.

| What | How |
| --- | --- |
| Frames | A `CADisplayLink` asking for up to 120 Hz ticks while something moves and pauses once a frame settles, so an idle view costs no CPU. Input, the app's `Waker` and the frame's `wake_after_secs` resume it. |
| Background | Every view stops its link while the app is in the background (Metal may not draw there), including a view made there, and resumes in the foreground. |
| Touches | `touchesBegan`/`Moved`/`Ended`/`Cancelled` go to `Runtime::touch`, with each move's coalesced samples, so taps, scrolls, flings, long presses and pinches behave as Rune's touch input describes. |
| Safe area | `UIView.safeAreaInsets` goes to `Runtime::set_safe_area`; read it with `Cx::safe_area`. |
| Appearance | Dark mode follows the trait collection; Reduce Motion follows `UIAccessibilityIsReduceMotionEnabled`. |
| Haptics | `Tx::haptic` plays through selection, impact and notification feedback generators attached to the view. |

It needs iOS 17.5. The app's `Info.plist` needs `UIApplicationSceneManifest` and
`CADisableMinimumFrameDurationOnPhone` (for 120 Hz on ProMotion iPhones); Rune's
`scripts/ios-app.sh <example> [--device <udid|name>]` writes both, bundles an example as a
simulator app, and installs and launches it.

Off iOS the crate builds empty.

Not yet: text input (`UITextInput`), the keyboard's insets, VoiceOver.
