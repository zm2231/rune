# swiftui-host

A SwiftUI app with a Rune screen in it. SwiftUI draws the tab bar, the navigation bar and its
buttons, a form in the second tab and a sheet; Rune draws the list of cards in the first tab.
The list fills the whole tab and pads itself by `Cx::safe_area`, so it rests between the bars
and scrolls under them.

The two sides talk through the C functions in `include/swiftui_host.h`, which `src/host.rs`
exports:

| Function | What it does |
| --- | --- |
| `swiftui_host_view` | A retained `UIView *` that draws the Rune app (`rune::ios::view`). |
| `swiftui_host_listen` | Gives Rune a context and two callbacks: the number of cards when it changes, and the card a tap opened. |
| `swiftui_host_add_card`, `swiftui_host_remove_card`, `swiftui_host_clear` | Change Rune's state through `AppHandle::update`. |

Every function runs on the main thread, and so do the callbacks.

## Build

From the repository root, on a Mac with Xcode:

```sh
scripts/ios-lib --package swiftui-host --module RuneCards --header examples/swiftui-host/include/swiftui_host.h
xcodebuild -project examples/swiftui-host/app/SwiftUIHost.xcodeproj -scheme SwiftUIHost \
  -destination 'generic/platform=iOS Simulator' -derivedDataPath target/ios/swiftui-host build
xcrun simctl install booted target/ios/swiftui-host/Build/Products/Debug-iphonesimulator/SwiftUIHost.app
xcrun simctl launch booted dev.rune.swiftuihost
```

`scripts/ios-lib` writes `target/ios/RuneCards.xcframework`, which the project links. The
project also adds the `ios/RuneView` Swift package for `RuneView`. Run `scripts/ios-lib` again
after changing the Rust side.

`cargo run -p swiftui-host` shows the Rune screen alone in a desktop window.
