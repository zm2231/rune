# Run a Rune app on iOS

Rune's experimental iOS target draws one `UIView` on a `CAMetalLayer` with the same Rust views,
layout, text and shaders as the desktop. A display link draws at up to 120 Hz while something
moves and stops when nothing does, so an idle app uses no CPU or GPU.

What works:

- **Drawing.** Light and dark appearance, the screen's own scale, up to 120 Hz, nothing drawn
  while idle or in the background.
- **Touch.** One finger taps, drags and pans with momentum; two fingers pinch (`on_magnify`)
  and turn (`on_rotate`); a long press is the secondary press, so it opens what a right-click
  opens on a desktop.
- **Pointer.** A mouse or trackpad on an iPad hovers, clicks, right-clicks and scrolls.
- **Safe area.** `Cx::safe_area` holds what the status bar, the home indicator and the soft
  keyboard take: pad a full-screen root by it.
- **Text input.** The soft keyboard shows while a text field has the focus. Typing, delete,
  return, predictions, autocorrection, dictation and marked text edit the field; a secure
  field shows the keyboard none of its text. The keyboard is told where the field's
  characters are, so the space bar's trackpad moves the caret.
- **Hardware keyboard.** Arrows, Return, Escape, Tab and the function keys with their
  modifiers, key repeat, and shortcuts with ⌘ as on a Mac. Every key is also a physical key,
  for `El::on_physical_input`.
- **App menus.** In an app `rune::run` started, `App::menu_item` items join an iPad's menu bar
  and its ⌘ list; on an iPhone their shortcuts run them when no handler takes the key.
- **Clipboard and menus.** Copy, cut and paste use the system pasteboard. A menu from
  `Tx::popup_menu` (a field's Cut, Copy and Paste among them) shows as the system's edit menu.
- **Accessibility.** VoiceOver, Switch Control and Voice Control read the same accessibility
  tree the desktop platforms do. The Reduce Motion setting is followed, and
  `Cx::system_text_scale` is the Dynamic Type size: `cx.zoom(cx.system_text_scale())` scales
  a view with it.
- **Handles.** `AppHandle::update` and `wake` from a background thread reach the app, as from
  a window; `is_visible` follows the app going to the background. `App::on_ready` runs once
  the first frame is on screen.
- **Links into the app.** `App::on_open_url` gets the links iOS opens with the app: its own
  schemes' (`myapp://...`) and universal links, the one that launched it among them. An app
  `rune::run` started gets them from iOS; one that embeds a Rune view passes them on with
  `rune::ios::open_url`. See "Links into the app" below.
- **Lifecycle hooks.** A view's scene tells the app what a window tells it on a desktop
  ([Lifecycle hooks](#lifecycle-hooks)).
- **Memory warnings.** When iOS says memory is short, the view rests at once, on screen or not:
  it rests as a window at rest does, `App::on_rest` included for the app's own caches, and then
  frees the caches its last frame didn't draw from (text layouts, glyphs, image and layer
  textures). Rune's part draws nothing and submits nothing to the GPU, so it does the same in
  the background and out of its window, and makes what went again when it next shows. An
  `on_rest` that changes what the view shows gets a frame at once when the view is in front. An
  app that embeds a `LayerHost` itself calls `LayerHost::trim_memory`.
- **Links and files.** `Tx::open_url` opens a link in the app that owns its scheme.
  A file dialog opens in its options' `directory` when the system can show that folder.
  `Tx::open_file` shows the system's document picker and answers with a copy of the file;
  `Tx::open_folder` answers with a folder the app may use until it quits, and `Tx::save_file`
  asks for the folder and answers with the dialog's file name inside it.

What doesn't yet:

- The system's selection handles and loupe: a field draws its own selection.
- An App's menu items in the menu bar of a UIKit or SwiftUI app that embeds a Rune view. That
  app's own delegate builds its main menu, so on an iPad only a `keys_first` item's shortcut
  runs there; on an iPhone every item's does.
- More than one surface an app: a `RuneView` is one view, so `App::window`'s other windows,
  `App::overlay`'s overlays and `rune::open_window`'s windows never open. Requests of a window
  are dropped: `AppHandle`'s `open`, `show`, `activate`, `hide`, `close`, `focus`, `minimize`,
  `set_position`, `place`, `set_dock_icon`, `quit` and `relaunch`, a handler's
  `Tx::close_window`, `Tx::minimize`, `Tx::quit` and `Tx::relaunch`, and the handle `on_ready`
  is given. The first dropped request of each kind prints one `rune: ... ignored: ...` line on
  stderr, which Xcode's console and `devicectl --console` show.
- Intel Macs' simulators: `scripts/ios-lib` builds for devices and Apple silicon simulators.

## Lifecycle hooks

A `RuneView` follows the scene of the window it is in, so on an iPad each of an app's scenes
tells its own view, and a view inside a UIKit or SwiftUI app hears the same as one `rune::run`
made:

| The system says | The app hears |
| --- | --- |
| the scene enters the foreground, or the view joins a window in front | `App::on_visibility(true)`; `AppHandle::is_visible` is true |
| the scene enters the background, or the view leaves its window | `App::on_visibility(false)`; `AppHandle::is_visible` is false. Then the view rests, and `App::on_rest` runs |
| the scene becomes active | `App::on_focus(true)` |
| the scene resigns active: Control Center, the app switcher, a call | `App::on_focus(false)` |
| the app will terminate | `App::on_focus(false)` and `App::on_visibility(false)` if it had not heard them, then `App::on_quit`, once. No rest |

Each hook hears a value once. Coming forward the app is visible before it has the focus, and
going away it loses the focus before it hides, which it has done by the time `on_quit` runs. As
in a window, `on_visibility` hears changes after the first frame, and `on_focus` hears the first
focus too. A composition the keyboard has open (Japanese, Pinyin) stays open while the focus is
away.

When the scene stops being active (and so before it hides, its view leaves the window or the
app terminates) what was held ends, in a desktop window's order: a drag under a finger hears
its cancelled `Phase::Up`, then every key held on a hardware keyboard is released, so
`El::on_physical_input` hears `PhysicalInput::FocusLost` and the key stops repeating. iOS
sends no key-up to an app it puts away.

A view that goes out of sight rests, as a desktop window does when it is hidden: once each time
its scene enters the background or it leaves its window, after `on_visibility(false)`. Rune's
caches go back to what the screen on show uses (the text layouts the last two commits didn't use
and their glyph runs, the images no element showed in the last frame, the elements a rewind
recording no longer refers to), then `App::on_rest` runs for the app's own caches (in an app
whose view has drawn its first frame, as `on_visibility` is heard), and the
renderer gives back everything it keeps between frames: `El::cache` textures no layer drew last
frame, spare and scratch layer targets, the backdrop copies and blur pyramid, and the staging
chunks finished frames have handed back. The first frame back in front makes again what it
draws with. All of it happens inside the system's notification, so keep `on_rest` quick; an app
in the background draws no frame to do it later. Nothing is submitted to the GPU there, and
nothing waits on it: the rest only drops what is held, since Metal takes no work from an app in
the background. An `on_rest` that returns true is shown by the first frame after the app comes
forward. iOS does not rest a view that is on screen and idle, as a desktop window rests after
10 s without a frame: there it rests only on a memory warning. An app that terminates does not
rest.

iOS announces termination only to an app that is running when it ends, never to a suspended one,
so `on_quit` is a last chance: save when `on_visibility` says false. An app ended while in front
loses the focus and hides before `on_quit`, which a desktop app that quits does not.
`App::on_close` and `App::on_reopen` are never called, since a view is no window to close and
iOS reopens no running app. `App::on_ime` hears the keyboard for an editor the app draws itself
(`El::ime_cursor`).

## Build and run in a simulator

Install the targets and Xcode with an iOS simulator runtime, then build any Rune binary and
hand it to `scripts/ios-app`:

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
cargo build -p hello --target aarch64-apple-ios-sim
scripts/ios-app --name Hello --id com.example.hello --bin target/aarch64-apple-ios-sim/debug/hello
```

The script stages `target/ios/Hello.app` (the binary and an `Info.plist`), signs it ad hoc,
installs it on the booted simulator and launches it. With no simulator booted the install
fails with `No devices are booted`: `--simulator NAME_OR_UDID` picks a simulator and boots it
first (`xcrun simctl list devices available` lists them). `--no-launch` and `--no-install` stop
earlier; arguments after `--` go to the app. No Xcode project is involved.

```text
usage: ios-app [-h] --name NAME --id ID --bin BIN [--exe EXE] [--out OUT]
               [--simulator SIMULATOR] [--device DEVICE] [--profile PROFILE]
               [--url-scheme SCHEME] [--no-install] [--no-launch]
               ...
```

## Before sending a change to the iOS code

Launch a debug build in a simulator once, and watch it reach its first frame:

```sh
cargo build -p web-hello --target aarch64-apple-ios-sim
scripts/ios-app --name WebHello --id com.example.webhello \
    --bin target/aarch64-apple-ios-sim/debug/web-hello --no-launch
xcrun simctl launch --console --terminate-running-process booted com.example.webhello
```

objc2 checks the classes Rune defines only in a debug build: that a block implementing a
protocol registers every required method of it, and only methods the protocol has. A class that
fails the check panics as it is registered, at launch, and the console shows which method. A
release build skips the check, so it launches and proves nothing about it. `web-hello` has a
text field, so the class the keyboard talks to is registered too.

## Run on a device

A device runs only what a development provisioning profile covers: one that lists the device
and whose app identifier matches `--id` (a wildcard profile matches any).

```sh
cargo build -p hello --target aarch64-apple-ios --release
scripts/ios-app --name Hello --id com.example.hello --bin target/aarch64-apple-ios/release/hello \
    --device DEVICE_UDID --profile path/to/development.mobileprovision
```

The script embeds the profile and signs with the profile's entitlements and the keychain's
`Apple Development` identity; set `IOS_APP_SIGN` to name another. `xcrun devicectl list devices`
lists paired devices. Xcode keeps the profiles it manages in
`~/Library/Developer/Xcode/UserData/Provisioning Profiles`.

## The app's entry point

The usual one. On iOS `rune::run` and `App::run` start UIKit, make the window and fill it with
the app, and never return:

```rust,ignore
fn main() {
    rune::run(AppSpec::new("My app", (390.0, 760.0), State::default, view));
}
```

`AppSpec`'s size is ignored: the app is the size of the screen. Move the app's own desktop-only
crates (a file dialog crate, hot reload, the system tray) behind target conditions; Rune's
file dialogs (`Tx::open_file`) need none.

## A Rune view in a UIKit or SwiftUI app

`rune::ios::view(app)` returns a retained `UIView *` that draws `app` and fills whatever it is
put in. Export a function that returns it from the app's library, and declare it in a C header:

```rust,ignore
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn my_screen() -> *mut std::ffi::c_void {
    rune::ios::view(rune::App::new(AppSpec::new("Screen", (390.0, 760.0), State::default, view)))
}
```

```c
void *my_screen(void);
```

`scripts/ios-lib` builds the library for devices and Apple silicon simulators and writes an
XCFramework that holds both, the header and a module map:

```sh
scripts/ios-lib --package my-app --module MyApp --header include/my_app.h
```

```text
usage: ios-lib [-h] --package PACKAGE --module MODULE --header HEADER
               [--out OUT] [--debug]
```

It builds with `cargo rustc --crate-type staticlib`, so the package's manifest needs no
`staticlib` crate type. The module map links the system frameworks Rune uses, so the host app
adds no linker flags. In Xcode, add `target/ios/MyApp.xcframework` to the app target's
frameworks and add the `ios/RuneView` Swift package, then:

```swift
import SwiftUI
import RuneView
import MyApp

struct Screen: View {
    var body: some View {
        RuneView(my_screen)
    }
}
```

`RuneView` is a `UIViewRepresentable`; a UIKit app takes the pointer itself with
`Unmanaged<UIView>.fromOpaque(my_screen()).takeRetainedValue()`. Call the function on the main
thread: it returns null anywhere else. The view reports the safe area of where it sits, so one
that doesn't reach the screen's edges reads zero. `examples/web-hello` exports
`web_hello_view` this way (`examples/web-hello/include/web_hello.h`).

## Links into the app

`App::on_open_url(|s, url: &str, tx| ..)` runs on the main thread for each link iOS opens with
an app `rune::run` or `App::run` started, as it does on a Mac. A link that launches the app
runs before the first frame; one that reaches the running app wakes it and redraws. Each runs
as an `AppHandle::update`, in the order iOS handed them over. Links it hands over together
(several at launch) come as sets with no order of their own: Rune runs the opened URLs before
the universal links. The string is the URL as iOS gave it: it comes from any app or web page,
so parse and validate it before acting on it.

**A scheme of the app's own.** `scripts/ios-app --url-scheme myapp` declares it in the bundle's
`Info.plist`; repeat the flag for more than one:

```sh
scripts/ios-app --name Hello --id com.example.hello --bin target/aarch64-apple-ios-sim/debug/hello \
    --url-scheme myapp
xcrun simctl openurl booted 'myapp://item/7'
```

On a device, open the link from Safari or Notes, or launch the app with it:
`xcrun devicectl device process launch --device DEVICE_UDID --payload-url 'myapp://item/7' com.example.hello`.
An app with its own bundle declares the same thing:

```xml
<key>CFBundleURLTypes</key>
<array>
  <dict>
    <key>CFBundleURLName</key>
    <string>com.example.hello</string>
    <key>CFBundleURLSchemes</key>
    <array><string>myapp</string></array>
  </dict>
</array>
```

**Universal links** (`https://example.com/item/7` opens the app instead of Safari) arrive at
the same hook with the web URL. Rune delivers them; the app and its web server say which
links they are, and `scripts/ios-app` sets none of this up:

1. Give the app's identifier the Associated Domains capability in the developer account, and
   make a provisioning profile that carries it.
2. Sign the app with the `com.apple.developer.associated-domains` entitlement listing each
   domain as `applinks:example.com`.
3. Serve `https://example.com/.well-known/apple-app-site-association` (no file extension, no
   redirect, `Content-Type: application/json`) naming the app and the paths it takes:

   ```json
   {"applinks": {"details": [{"appIDs": ["TEAMID.com.example.hello"], "components": [{"/": "/item/*"}]}]}}
   ```

iOS fetches that file through Apple's servers when the app is installed, so a new domain can
take a while to work. A link typed into Safari's address bar stays in Safari; one tapped in
another app or on another site's page opens the app.

**A Rune view in another app** gets no links from iOS: that app's own scene delegate is handed
them. It passes each on with `rune::ios::open_url(url)`, which runs `on_open_url` exactly as
above. With more than one Rune view in an app, a link that arrives before the views are made
goes to the first view made, and is dropped if that view's app has no `on_open_url`. Export a
function for it beside the one that makes the view, and declare it in the header:

```rust,ignore
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn my_open_url(url: *const std::ffi::c_char) {
    // SAFETY: the caller passes a NUL-terminated string that lives until this returns.
    if let Ok(url) = unsafe { std::ffi::CStr::from_ptr(url) }.to_str() { rune::ios::open_url(url); }
}
```

```c
void my_open_url(const char *url);
```

A SwiftUI app calls it from `onOpenURL`, which gets both kinds of link:

```swift
RuneView(my_screen)
    .onOpenURL { url in my_open_url(url.absoluteString) }
```

A UIKit app calls it from its scene delegate: with each context's `url.absoluteString` in
`scene(_:openURLContexts:)`, with the activity's `webpageURL` in `scene(_:continue:)`, and
with both from the connection options in `scene(_:willConnectTo:options:)`. The first view
made whose app has the hook gets the link. One passed on before any view is made waits for
the first view; once a view is made, a link no view has a hook for is dropped.

## What the `Info.plist` needs

`scripts/ios-app` writes these, with the bundle's name, identifier, executable and versions.
An app with its own bundle needs the same:

- `UIApplicationSceneManifest` with `UIApplicationSupportsMultipleScenes` false and no
  configurations: Rune supplies the scene's in code, and shows one window.
- `UILaunchScreen` (an empty dictionary will do), or iOS letterboxes the app.
- `CADisableMinimumFrameDurationOnPhone` set to true, or an iPhone holds the app at 60 Hz.
- `MinimumOSVersion` 16.0 or later.
- `UIDeviceFamily` `[1, 2]` (iPhone and iPad): without the 2 an iPad runs the app as an
  iPhone app, in a phone-sized frame.
- `UISupportedInterfaceOrientations` with portrait and both landscapes, for an iPhone.
- `UISupportedInterfaceOrientations~ipad` with all four orientations, or an iPad runs the app
  full screen only, never in Split View or a Stage Manager window.
- `CFBundleURLTypes` with the app's schemes, when it takes links (`--url-scheme`).
