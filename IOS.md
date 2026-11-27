# Run a Rune app on iOS

Rune's experimental iOS target draws one `UIView` on a `CAMetalLayer` with the same Rust views,
layout, text and shaders as the desktop. A display link draws at up to 120 Hz while something
moves and stops when nothing does, so an idle app uses no CPU or GPU.

What works:

- **Drawing.** Light and dark appearance, the screen's own scale, up to 120 Hz, nothing drawn
  while idle or in the background.
- **Appearance.** `App::color_scheme`, `Tx::set_color_scheme` and `AppHandle::set_color_scheme`
  set the view's own light or dark appearance (its `overrideUserInterfaceStyle`), as a window's
  scheme sets a desktop window's: a view's Light or Dark wins over the app's, and System follows
  the app's, then the system's. In an app `rune::run` or `rune::run_windows` started, Rune's
  view controller takes the same appearance, and the status bar's text and icons follow it:
  light on a dark appearance, dark on a light one. A view embedded in a UIKit or SwiftUI app
  changes only itself; the host's controller, and so its status bar, stay the host's.
- **Materials.** `El::system_material` blurs what the view drew behind the element, under a tint
  near the material's own in light and dark, as UIKit's materials blur an app's content: Glass
  is the clearest, Sidebar the most opaque. A translucent fill is the tint instead, and an
  opaque fill draws alone, unblurred, as on Windows and Wayland.
- **Touch.** One finger taps, drags and pans with momentum; two fingers pinch (`on_magnify`)
  and turn (`on_rotate`); a long press is the secondary press, so it opens what a right-click
  opens on a desktop.
- **Scroll to top.** When UIKit tells the view of a tap on the status bar, the app's main list
  glides to its top, or jumps there with Reduce Motion on. The main list is the first vertical
  scroll view down the view's centre line from the status bar: the list under a title bar, the
  column under the centre where columns sit side by side, a pager's showing page, a modal's own
  list rather than the one behind it, and the outermost of nested lists that has anywhere to go.
  A list already at its top, and a view with no list there, do nothing; a list that follows its
  end (`stick_bottom`) stops following it. This was shown on the iOS simulator; a real tap on a
  device is not yet verified. See "The main list and UIKit's bars" below.
- **Pointer.** A mouse or trackpad on an iPad hovers, clicks, right-clicks and scrolls.
- **Safe area.** `Cx::safe_area` holds what the status bar, the home indicator and the soft
  keyboard take: pad a full-screen root by it. When the keyboard rises over the focused text
  field, or moves, the field's scroll views scroll the whole field into what the keyboard leaves.
- **Size classes and multitasking.** `Cx::horizontal_size_class` and `Cx::vertical_size_class`
  are the view's trait collection: compact across on an iPhone and in a narrow iPad window
  (Slide Over, a third of Split View, a small Stage Manager window), regular on a full-screen
  iPad. The view takes every size its window is given, and a resize or rotation that changes a
  class re-runs the view. A bundle `scripts/ios-app` makes turns every way on an iPad, which
  is what lets it share the screen.
- **Text input.** The soft keyboard shows while a text field has the focus. Typing, delete,
  return, predictions, autocorrection, dictation and marked text edit the field; a secure
  field shows the keyboard none of its text. The keyboard is told where the field's
  characters are, so the space bar's trackpad moves the caret.
- **Selection handles and loupe.** On iOS 17 and later, text selected in the focused field
  (Select or Select All) shows the system's handles. A finger on a handle drags that end of
  the selection under the system's loupe; the field's menu hides while it does and opens again
  for what is selected when the finger lifts. A finger held on the focused field's text moves
  the caret under the loupe, and its menu opens when the finger lifts, as in a UIKit field.
  The menu sits clear of the handles, so a finger on either one reaches it. The field draws
  its own caret and highlight; on iOS 16 there are no handles or loupe.
- **Keyboard traits.** The focused field answers UIKit's `UITextInputTraits`: `El::input_mode`
  is its `keyboardType` (a number pad on an iPhone, the decimal and phone pads, the email, URL
  and search keyboards), `El::autocomplete` its `textContentType`, and `El::autocapitalize`,
  `El::autocorrect` and `El::enter_key_hint` its capitalization, autocorrection and Return key.
  A secure field's content type is a password, or a new password. With a content type the
  system may offer a saved password, a strong new one or a code from a message; whether it
  does is its decision. The keyboard is reloaded when the focused field's traits change,
  after any marked text is closed. A field that sets none of them answers UIKit's defaults.
- **Spell checking.** A field with `El::spellcheck(true)` gets a row of red dots under each
  misspelled word the caret has left, from UIKit's `UITextChecker`, and its long-press menu
  leads with the word's suggestions and Learn Spelling, as a right-click does on a Mac. See
  "Spell checking" below.
- **Undo gestures.** A shake, the three-finger swipes and the three-finger tap's bar undo and
  redo the focused field's own history, and on an iPad so do Edit › Undo and Redo in the menu
  bar. With no field focused nothing is offered. See "Undo gestures" below.
- **System fonts.** Text draws in the fonts the app bundles (Inter by default).
  `Family::Named("System Font")` is San Francisco and `Family::SystemMono` SF Mono, loaded
  from the files of the system's UI font the first time a text asks for them. A character the
  app's fonts lack (emoji, Arabic, Hebrew, Devanagari, Thai, kana, hangul...) draws in the
  font CoreText falls back to for it, loaded the same way. Emoji draw in colour: Apple Color
  Emoji's images on iOS are in a compressed format of Apple's, so Core Text draws each one,
  from the font already in memory. On the iOS simulator a smiley, a family and a flag drew
  in colour at 17 pt and 48 pt, each as one glyph; the first emoji took under a frame (6 ms
  at worst in a release build, a later one under half a millisecond), and the app's footprint
  grew by about 6 MB, the font's 139 MB staying a mapped file. Not yet verified on a device.
  `text::ink` measures an emoji through swash alone, which can't read those images, so on
  iOS a text of emoji only reports no ink and a text with emoji is measured by its other
  characters; that matters only to an app that centres a text by its drawn ink.
- **Accent.** `Cx::accent` is the view's `tintColor` and `Cx::focus_color` that colour at half
  its alpha. A view inherits its tint from the views above it, which is where a UIKit host's
  `tintColor` and a SwiftUI host's `.tint()` land; where none is set it is iOS's system blue, as
  in an app `rune::run` started. The accent is whatever UIKit reports as the view's tint,
  including its dimmed tint while an alert or sheet is over the view. A tint that differs in
  dark or with Increase Contrast follows them, and the view redraws when its accent changes. A
  colour outside sRGB (Display P3) is clipped to sRGB. `rune::system_accent()` knows no view, so
  on iOS it answers the built-in default blue, not the tint.
- **Hardware keyboard.** Arrows, Return, Escape, Tab and the function keys with their
  modifiers, key repeat, and shortcuts with ⌘ as on a Mac. Every key is also a physical key,
  for `El::on_physical_input`.
- **App menus.** In an app `rune::run` started, `App::menu_item` items join an iPad's menu bar
  and its ⌘ list; on an iPhone their shortcuts run them when no handler takes the key.
- **Share sheet.** `Tx::share` shows `UIActivityViewController` with the share's text, link
  and files. On an iPad it is a popover pointing at the share's anchor (an element's key or a
  point), or at the view's centre when that element is gone. The callback gets the activity
  the user chose (`com.apple.UIKit.activity.Mail` and the like), or `None` when they closed
  the sheet.
- **Clipboard and menus.** Copy, cut and paste use the system pasteboard. A menu from
  `Tx::popup_menu` (a field's Cut, Copy and Paste among them) shows as the system's edit menu.
- **Accessibility.** VoiceOver, Switch Control and Voice Control read the same accessibility
  tree the desktop platforms do: a double tap presses a control, a swipe up or down adjusts a
  slider, a three-finger swipe pages the scroll view under the cursor and says which page it
  landed on, the cursor follows the app's focus, live regions and `Tx::announce` are spoken, and
  a control's context menu is in the Actions rotor. The Reduce Motion setting is followed.
  `Cx::system_text_scale` is the Dynamic Type size, `Cx::system_bold_text` the Bold Text
  setting and `Cx::system_increased_contrast` Increase Contrast: `cx.zoom(cx.system_text_scale())`
  scales a view with the text size, and `cx.bold_text(cx.system_bold_text())` draws its text a
  weight heavier, as UIKit's text is.
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
  `on_rest` that changes what the view shows gets a frame at once when the view is in front. A
  view out of its window (a hidden window, a dismissed sheet, a hidden overlay) also gives back
  its renderer and its drawables, as a desktop window that releases its GPU when hidden does,
  and makes them again with its next frame. An app that embeds a `LayerHost` itself calls
  `LayerHost::trim_memory`.
- **Windows and overlays.** An app's other windows and its overlays show
  ([Windows and overlays](#windows-and-overlays)).
- **Links and files.** `Tx::open_url` opens a link in the app that owns its scheme, and
  `Tx::open_url_then` hears whether one took it, on the frame after the system says.
  A file dialog opens in its options' `directory` when the system can show that folder.
  `Tx::open_file` shows the system's document picker and answers with a copy of the file.
  `Tx::open_folder` answers with the folder picked, and `Tx::save_file` asks for a folder and
  answers with the dialog's file name inside it. After either, the app may read and write in
  the picked folder until it quits: Rune starts the access the pick grants and never stops
  it, and it ends with the process. It doesn't carry to the next launch, so ask again then.
  A save asks nothing before it replaces: the answer is only a path, and a file of that name
  already in the folder is replaced by what the app writes there. A dialog's `title` isn't
  shown: the picker has none.
  `Tx::pick_media` shows the system's photo picker, which asks for no permission to the photo
  library: each photo or video picked is copied into the app's temporary folder, which the
  system may empty while the app isn't running, and the app gets the copies' paths in the order
  picked.
  An iOS app has no Downloads folder, so `AppDirs::downloads` is `None`: save through
  `Tx::save_file`.

What doesn't yet:

- An App's menu items in the menu bar of a UIKit or SwiftUI app that embeds a Rune view. That
  app's own delegate builds its main menu, so on an iPad only a `keys_first` item's shortcut
  runs there; on an iPhone every item's does.
- What no iOS window does: `AppHandle`'s `minimize`, `set_position`, `place`, `set_dock_icon`,
  `quit` and `relaunch`, and a handler's `Tx::minimize`, `Tx::quit` and `Tx::relaunch`, are
  dropped. A Rune view a UIKit or SwiftUI app embeds is no window either, so there `open`,
  `show`, `activate`, `hide`, `close` and `focus`, `Tx::close_window`, `App::window`'s
  windows and `rune::open_window`'s are dropped too. The first dropped request of each kind
  prints one `rune: ... ignored: ...` line on stderr, which Xcode's console and
  `devicectl --console` show.
- Overlays' desktop-only options: `DisplayTarget` and `follow_pointer` (a scene has one
  screen), `OverlayHandle::send_pointer` and `screen_to_local`, capture exclusion, the window
  number and activation tokens (answered `None`).
- Chinese in the system's own font. iOS's Chinese font is PingFang, whose glyphs are in a
  table (`hvgl`) Rune has no reader for. A Chinese character goes to the first other font in
  CoreText's cascade that has it (a Japanese font, for a character Japanese shares), and to no
  font when there is none. An app with Chinese text bundles a font for it (`AppSpec::fonts`).
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
`App::on_close` runs when the user closes a window ([Windows and
overlays](#windows-and-overlays)); a view a UIKit app embeds never hears it. `App::on_reopen` is
never called: iOS reopens no running app. `App::on_ime` hears the keyboard for an editor the app draws itself
(`El::ime_cursor`).

## Windows and overlays

An app `rune::run_windows` or `rune::run` starts has every window its factory returns, its apps
add with `App::window`, and its handlers open with `rune::open_window`. The first is the
primary. A window keeps its view and its state from the first time it shows until the app
ends: hiding or closing it takes it off screen, and showing it again brings back the same
state, as `App::window`'s windows do on the desktop. `AppHandle`'s and `WindowHandle`'s `open`,
`show`, `activate`, `hide`, `close` and `focus`, and `Tx::close_window`, act on it.

**iPad.** An app whose bundle declares several scenes (`scripts/ios-app --multiple-scenes`,
which sets `UIApplicationSupportsMultipleScenes` and declares the `rune.window` activity type
in `NSUserActivityTypes`) shows each window in a scene of its own, which the user arranges as
any app's windows: side by side, in Stage Manager, or one at a time. Showing a window asks the
system for its scene, focusing one on screen brings its scene forward, and hiding or closing
one takes the window out of its scene and asks the system to end the scene. When the window
hidden or closed is the only one of the app's on screen, the window the user last had in front
comes forward first (the primary when no other is left, asked for again if the user closed
it), so the app stays on screen instead of going to the Home Screen; the closed window's
scene is asked to end once the system has moved it behind, and `App::on_close` runs then.
Asking to hide or close it again before that does both at once. iPadOS may keep a scene it
was asked to end, in the background and in the app switcher: showing the window again then
brings that scene back with the window in it, and a kept scene the user brings back from the
switcher gets its window too. A scene's session carries its window's key, so when the system
restores an app's scenes at launch each comes back with its window. Windows that start visible
(`WindowOptions::visible`) open after the primary draws its first frame, on a fresh launch
only: once the system restores scenes, it decides which windows come back, and a window the
user closed stays closed. A restored scene whose window the app no longer has is asked to end.

**iPhone, and an iPad app with one scene.** The primary fills the scene and can't hide: a
request to hide or close it is dropped. Every other window shows as a page sheet over the
window on top, stacked in the order they showed; windows that start visible are presented
after the primary's first frame on every launch. Hiding a sheet that has others above it
presents those again over the one below.

**The user closing a window** (closing its scene for good, or swiping its sheet down) runs its
`App::on_close`, as on the desktop. A scene the system disconnects to save memory is not
closed: its window comes back when the system connects the scene again.

**Overlays.** `App::overlay`'s overlays show over the window of the app that made them, above
its content and in `OverlayLevel` order, and go with it: off screen with a hidden window, into
the background with its scene. An overlay a Rune view a UIKit app embeds makes shows over that
view. Placement is in the window's points:

- A screen edge (`OverlaySpec`'s edge, margin, align and inset, or `OverlayPlacement::Edge`)
  measures from the safe area, so a top overlay sits below the status bar or Dynamic Island
  and a bottom one above the home indicator and the keyboard.
- `OverlayPlacement::Rect` is in the window's points, the space `El::on_screen_layout` reports.
- `OverlayPlacement::FullScreen` covers the window.
- A content-sized overlay (`ContentSizing`) takes the size its content measures.

It moves with the window's rotation and resizing. `show`, `hide`, `fade_in` and `fade_out`
show and hide it, fading its alpha. A touch goes through an overlay that takes none
(`OverlayInput::None`, or a bare spec that isn't `interactive`) to the window under it; one
that takes all (`OverlayInput::All`, or an `interactive` spec) keeps every touch in its rect;
`OverlayInput::Regions` keeps only touches on its `El::hit` elements. An overlay with
`KeyPolicy::OnDemand` or `Exclusive` takes a hardware keyboard on `request_keys` or a touch,
and `release_keys` gives it back to the window. `Availability` is `Available` once the overlay
first shows. VoiceOver reads a shown overlay's elements after the window's.

The pop-overs, menus, selects, hover cards and toasts of `rune-kit` draw inside the window
(`El::anchored`) and are no overlays: they look and behave the same on every platform.

## Build and run in a simulator

Install the targets and Xcode with an iOS simulator runtime, then build any Rune binary and
hand it to `scripts/ios-app`:

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
cargo build -p hello --target aarch64-apple-ios-sim
scripts/ios-app --name Hello --id com.example.hello --bin target/aarch64-apple-ios-sim/debug/hello
```

The script stages `target/ios/Hello.app` (the binary, an `Info.plist`, a privacy manifest
and, with `--icon`, the icon's files), signs it ad hoc,
installs it on the booted simulator and launches it. With no simulator booted the install
fails with `No devices are booted`: `--simulator NAME_OR_UDID` picks a simulator and boots it
first (`xcrun simctl list devices available` lists them). `--no-launch` and `--no-install` stop
earlier; arguments after `--` go to the app. No Xcode project is involved.

```text
usage: ios-app [-h] --name NAME --id ID --bin BIN [--exe EXE]
               [--display-name TEXT] [--version X.Y.Z] [--build N]
               [--icon PNG] [--privacy FILE] [--out OUT]
               [--simulator SIMULATOR] [--device DEVICE] [--profile PROFILE]
               [--distribution] [--min-os X.Y] [--url-scheme SCHEME]
               [--activity-type TYPE] [--multiple-scenes]
               [--no-install] [--no-launch] [--wait SECONDS]
               ...
```

An app can die as it starts in two ways, and the script fails on both. The app launches and
is gone a moment later: `--wait SECONDS` after the launch (3 unless told; `--wait 0` skips
this check) the launched process must still be running, or the script exits non-zero and
says where the crash log is. Or the launch itself fails, because the app died before it had
a process: the script exits non-zero and says where the crash log is, with what the tool
said. A check that passes proves that much and no more: the process is still running after
the wait. On a simulator that has just booted the app can still be on its way to the screen
then. A simulator's app is a process of the Mac, so the script asks `ps` for the process id
`simctl launch` printed, and macOS writes the crash log to
`~/Library/Logs/DiagnosticReports/<executable>-YYYY-MM-DD-HHMMSS.ips`; the script names the
newest one there.

### The deployment target

Rune runs on iOS 16 and later: its Metal backend writes MSL 3.0. The binary and the bundle
each say the oldest iOS the app runs on, and they have to say the same. The binary's comes
from `IPHONEOS_DEPLOYMENT_TARGET` when it is built. Without it rustc links a device build for
iOS 10, where a debug build fails to link (`___chkstk_darwin`, from blake3's NEON objects),
and a simulator build for iOS 14. Rune's own `.cargo/config.toml` sets it, which is why the
commands here need nothing more. A workspace config is not inherited by an app that depends
on Rune, so the app sets it in its own `.cargo/config.toml`:

```toml
[env]
IPHONEOS_DEPLOYMENT_TARGET = "16.0"
```

or in the shell, which wins over the file:

```sh
IPHONEOS_DEPLOYMENT_TARGET=16.0 cargo build -p my-app --target aarch64-apple-ios --release
```

The bundle's is `MinimumOSVersion`, which `scripts/ios-app --min-os X.Y` writes (16.0 unless
told; below 16.0 is refused). Before signing, the script reads what the staged binary was
built for (`xcrun vtool -show-build`) and stops when it is not `--min-os`, whichever of the
two is higher:

```text
ios-app: target/aarch64-apple-ios-sim/debug/hello was built for the iOS simulator 14.0 but the bundle says 16.0: build with IPHONEOS_DEPLOYMENT_TARGET=16.0 (see IOS.md)
```

That is all the script checks of `--bin`. A Mach-O object file with the executable bit set
names a build version too, and is staged as if it were an app's binary. A fat binary whose
architectures are for iOS and for another platform is refused as one whose `architectures
were not built for one platform and minimum OS`, not as one that is not for iOS.

`scripts/ios-lib` builds with `IPHONEOS_DEPLOYMENT_TARGET=16.0` unless the caller set it, and
refuses a value below 16.0. A static library is not linked, so its objects each keep a
minimum of their own ("A Rune view in a UIKit or SwiftUI app" below).

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

The scripts' own tests are run by hand, and need no Xcode:

```sh
python3 scripts/ios_bundle_tests.py
python3 scripts/ios_app_tests.py
python3 scripts/ios_lib_tests.py
```

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

`--wait` checks a device's launch too: the script reads the process id from the launch's JSON
result and looks for it in `devicectl device info processes`. The device keeps its own crash
logs, so for an app that died the script prints the commands that list them and copy one to
the Mac:

```sh
xcrun devicectl device info files --device DEVICE_UDID --domain-type systemCrashLogs
xcrun devicectl device copy from --device DEVICE_UDID --domain-type systemCrashLogs \
    --source FILE --destination .
```

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
`staticlib` crate type. The crates' objects in the library say iOS 16.0; the objects of
Rust's prebuilt standard library keep the minimum they were built with (10.0 in the device
slice, 14.0 in the simulator's). That is no disagreement of the kind "The deployment
target" above refuses: the minimum that has to match a bundle's is the one the linked binary
names, which the host app's own link sets. The module map links the system frameworks Rune
uses, so the host app adds no linker flags. The script also writes
`target/ios/PrivacyInfo.xcprivacy`, Rune's privacy manifest ("The privacy manifest" below):
the `ios/RuneView` package carries the same file as a resource, so only a host app that
doesn't use the package adds this copy to its target, or merges it into a manifest of its
own. In Xcode, add `target/ios/MyApp.xcframework` to the app target's frameworks and add the
`ios/RuneView` Swift package, then:

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

`examples/swiftui-host` is a whole SwiftUI app with an Xcode project. It puts a Rune list
inside a native tab bar and navigation bar. The toolbar's buttons change Rune's state through
`App::handle`, and a tap in Rune opens a SwiftUI sheet through a callback the app registers.

## The main list and UIKit's bars

UIKit looks for a scroll view in two places where a Rune list needs it to: a tap on the status
bar, which it sends to scroll views and not to views, and the bars of a `UINavigationController`
or `UITabBarController` (or SwiftUI's `NavigationStack` and `TabView`), which blur and dim what
scrolls under them only when it sits in a scroll view. A `RuneView` keeps one for both: a
scroll view that fills it, draws nothing, takes no touch and no reader sees. After each frame
it holds the main list's place: its content is the list's length, and its offset is the list's
offset less the safe area's top, so it rests at the top of its content when the list does.
Asked to scroll to its top, it stays where it is and `Runtime::scroll_main_to_top` scrolls
the app's list.

The main list is the first vertical scroll view down the view's centre line from under the
status bar (the **Scroll to top** item above), and `Runtime::main_scroll` reads its offset and reach. With
no such list the scroll view's content fits it, and the bars show nothing under them.
When the view joins a window, its nearest view controller is given this scroll view for each
of its top and bottom edges that names none of its own (`setContentScrollView(_:for:)`), so
its bars follow the main list for their large title as well as their edge. An edge where the
host named a scroll view keeps it. When the view moves to another superview or window
and so to another controller, or leaves its window, the edges it was given are cleared and
the new controller is offered its vacant ones. Moving a view's ancestor between controllers in
the same window tells the view nothing, and leaves the edges where they were.

What the bars did was shown on the iOS 27 simulator, under SwiftUI's `NavigationStack` and
`TabView` (`examples/swiftui-host`):

- with the list at its top the navigation bar showed no edge, and it blurred the cards that
  scrolled under it, up under the status bar;
- a large title collapsed into the bar as the list was dragged up, stayed collapsed when the
  finger lifted, and opened again back at the top;
- around the floating tab bar, the Rune list looked as a native SwiftUI `List` did at the same
  place in its scroll: neither showed a strong bottom edge.

When the safe area's top changes, as a large title collapses into the bar or opens out of it,
the main list's offset moves by the same amount (`Runtime::shift_main_scroll`), so what it
shows stays where it was on screen and under a finger that holds it. A list at rest at its
top stays at its top.

What UIKit does with a tap on the status bar was shown on the iOS simulator only:

- UIKit asked every scroll view under the tap's x, Rune's and a host's own among them, and a
  host's scroll view that agreed scrolled to its top as the app's list did;
- a scroll view that didn't reach the tap's x wasn't asked, and one that sat below the
  status bar was.

A real tap on a device is not yet verified. Nor is a Rune view in one column of a split
view, which by the above would scroll for a tap above that column only.

The scroll view is told only that the status bar was tapped, not where. A Rune view that
draws several columns itself scrolls the one under its centre, wherever above it the tap was.

## Spell checking

`UITextChecker` checks one language at a time, where AppKit's checker finds each text's
language itself. So iOS checks as Windows does: with the dictionaries `rune::spell::languages()`
names, and a word is marked when every one of them calls it misspelled. Unless the app chose
(`rune::spell::set_languages`, or a field's Spellcheck Languages menu where the app has
`El::on_spellcheck_languages`), those are one dictionary for each of the user's languages
(`NSLocale.preferredLanguages`, in order) that `UITextChecker.availableLanguages` has, matched
by its whole tag, else by its bare language (`de-AT` to `de`), else the first dictionary of
that language (`es-419` to `es-ES`), and US English as well when none of them is English.
UIKit writes a dictionary as `en_US`; Rune's tags are `en-US`.

The checker is asked on the main thread, which is where UIKit keeps it: `available_languages`
answers an empty list from any other thread. Each word is checked once and its answer kept,
so a key typed into a long draft checks the word it changed. A field's paint gives the
checker at most 32 words it has not seen and asks for another frame while the field has
more, so a long text's marks fill in from its start over frames, and a field whose words
are all checked asks for nothing. A word over 128 bytes (a URL, pasted data) is not checked.
The first check loads the dictionary, and a menu's suggestions are asked for as it opens,
both on the main thread, as on a Mac. Learn Spelling adds the word to the words UIKit has
learned, for every language.

On the iOS simulator, a field that started with `helo 🙂 wrold café 日本語 mispeled` showed
dots under `helo`, `wrold` and `mispeled` only; a long press on `wrold` offered `world` and
`wold`, picking `world` replaced the word, and Learn Spelling on `mispeled` cleared its dots.
A field of 2,000 different words filled in a batch of words a frame, after which the view
drew no more; in that debug build the first check, which loads the dictionary, took 0.47 s.
That run gave the checker 64 words a frame; with 32 it has not been run.
None of this is yet verified on a device, nor is any language but English.

## Undo gestures

UIKit offers undo and redo when the first responder's `undoManager` says it can do them, and
does them by asking that manager. While a text field has the focus the keyboard's text input
answers with a manager of its own, and with none otherwise. The field's history
(`TextEdit::can_undo`, `can_redo`) stays the only history: the manager holds at most one undo
action and one redo action, there so that `canUndo` and `canRedo` say what the field would,
and running one sends the field ⌘Z or ⇧⌘Z. After each frame what the manager offers is
compared with the field and, when they differ, the manager is emptied and filled again. What
it offers is kept from the last time it was filled: the manager itself is asked only on the
frame after it was filled or ran an action, so a frame that changes nothing sends it nothing.
When the text input goes away its manager is emptied, so no action is left to call it.

A manager can redo only what it has undone, so one that should offer a redo is given an undo
action and told to undo it; an action that runs then sends the field nothing. If that undo
runs no action, the next frame finds the manager short of the field and fills it again. The
actions are registered one to a group, with the manager's grouping by event off.

Nothing is offered or run twice. The field's long-press menu has no Undo or Redo. An iPad's
Edit › Undo and Redo are UIKit's own items, which act through the same manager. A hardware
keyboard's ⌘Z reaches the field once.

On the iOS simulator a shake offered Undo, Redo or both as the field had them, and each
choice, each of the iPad menu bar's Undo and Redo and each ⌘Z took one step of the field's
history. The first Undo chosen on a fresh simulator brought up UIKit's hint about the
three-finger double tap, and the text stayed as it was until the next. The three-finger
gestures need three fingers, which the simulator was not given: they and a real shake are
not yet verified on a device. Nor is it known whether, on a device, the undo that fills the
manager with a redo shows UIKit's undo HUD each time the manager is filled.

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

## Handoff and Spotlight

**Handoff.** `tx.advertise(Activity::new("com.example.hello.viewing").with_title(..).with_info(..))`
offers what the window shows to the user's other devices, and `App::on_continue_activity` gets an
activity continued here, the one that launched the app before the first frame. Each window has
its own activity, which becomes current while the window is in front; `tx.stop_advertising()`
retires it. iOS continues only the types the bundle lists in `NSUserActivityTypes`:
`scripts/ios-app --activity-type com.example.hello.viewing` adds one (repeat the flag for more),
beside the `rune.window` type `--multiple-scenes` lists. Handoff needs two devices signed in to
the same Apple Account, with apps from the same team; a simulator can't hand off.

**Spotlight.** `tx.index_search_items([SearchItem::new("note-7").with_title("Groceries")])` adds
items to the device's search, and `App::on_open_search_item` gets the id of a result the user
opens, the one that launched the app before the first frame. It needs nothing in the bundle and
works on a simulator: index, swipe down on the home screen, search, and open the result. The
home-screen search finds an item by the words of its title, not its details or keywords, and may
not match a search of three letters or fewer.
`examples/ios-system` does both:

```sh
cargo build -p ios-system --target aarch64-apple-ios-sim
scripts/ios-app --name System --id dev.rune.ios-system --bin target/aarch64-apple-ios-sim/debug/ios-system \
    --activity-type dev.rune.ios-system.viewing
```

## Voice calls

`rune::Voice` (see "Voice calls" in `crates/rune/README.md`) runs a call through Apple's
voice-processing I/O unit. While a voice stream runs, it owns the app's audio session:

- **Category and mode**: `AVAudioSessionCategoryPlayAndRecord` in `AVAudioSessionModeVoiceChat`,
  with `AllowBluetoothHFP` (a Bluetooth headset's microphone works) and `DefaultToSpeaker` (the
  loudspeaker, not the earpiece, when nothing is plugged in). The session asks for 48 kHz and
  10 ms buffers. The voice unit converts to the stream's own rate.
- **Activation**: `Voice::start` activates the session and `Voice::stop` deactivates it with
  `NotifyOthersOnDeactivation`, so music that the call paused resumes.
- **Interruptions**: a phone call, an alarm or Siri pauses the stream (`VoiceState::Interrupted`,
  `VoiceEvent::Interrupted`). When the interruption ends with the system's resume hint, the
  stream starts again (`VoiceEvent::Resumed`). Without the hint, it starts again the next time
  the app comes to the foreground. A stream stopped during an interruption stays stopped.
- **Routes**: plugging in or unplugging headphones, or a headset connecting or going away,
  rebuilds the stream on the new route (`VoiceEvent::DeviceChanged`). A media services reset
  rebuilds it too.
- **The background**: a call keeps running when the app goes to the background. The stream
  runs on its own threads and the audio thread, not on the view, so the view resting does not
  touch it. iOS keeps the app running only when `UIBackgroundModes` includes `audio`;
  without it, iOS suspends the app and the call stops until the app comes back.
- **Permission**: iOS asks the first time the session records. Ask earlier with
  `Voice::request_permission`. `Voice::permission` reads the answer without asking.

The `Info.plist` of an app that uses `Voice` needs:

- `NSMicrophoneUsageDescription`, the sentence iOS shows when it asks. Without it,
  `Voice::open` returns `VoiceError::OpenFailed` rather than let iOS end the app.
- `UIBackgroundModes` with `audio`, for a call that keeps going in the background.

## Ship an app

What `scripts/ios-app` puts in a bundle beyond what a test run needs, and the build it makes
for the App Store. A bundle to ship is built for the deployment target it names ("The
deployment target" above), has its own versions, an icon and a privacy manifest that covers
the app, and is made with `--distribution`.

### Versions and names

```sh
scripts/ios-app --name Hello --id com.example.hello --bin target/aarch64-apple-ios/release/hello \
    --display-name "Hello World" --version 1.2.0 --build 42 --no-install
```

| Flag | `Info.plist` key | Unless told |
| --- | --- | --- |
| `--name` | `CFBundleName`, and the bundle is `<name>.app` | required |
| `--display-name` | `CFBundleDisplayName`, the name under the icon | `--name` |
| `--version` | `CFBundleShortVersionString`, the version users see | `0.0.1` |
| `--build` | `CFBundleVersion`, the build's own number | `1` |
| `--id` | `CFBundleIdentifier` | required |
| `--exe` | `CFBundleExecutable`, the binary's name in the bundle | the binary's file name |

`--name` and `--exe` are file names: an empty one, one with a `/` in it and one that starts
with a dot are refused.

A version and a build number are each one to three whole numbers joined by dots (`7`, `1.2`,
`1.2.3`), 18 characters at most; anything else is refused, since the App Store refuses it.
The App Store also wants each upload of one version to carry a higher build number than the
last, which is the app's to keep count of.

### The icon

`--icon PNG` takes one image, 1024×1024 and opaque, and the system makes every size from it:

```sh
scripts/ios-app --name WebHello --id com.example.webhello \
    --bin target/aarch64-apple-ios-sim/debug/web-hello --icon examples/web-hello/ios/icon.png
```

The script reads the PNG's header itself and refuses a file that is not a PNG, one of another
size, and one with an alpha channel or a transparency chunk (`tRNS`), since the App Store
rejects an icon that can be see-through. A failed compile shows what `actool` said.
Without `--icon` the bundle has no icon and iOS shows a blank one.

It stages an asset catalog beside the bundle and compiles it into the bundle with
`xcrun actool`, for the platform the binary was built for and for `--min-os`:

| File | What it is |
| --- | --- |
| `<out>/<name>.assets/Assets.xcassets/Contents.json` | the catalog |
| `<out>/<name>.assets/Assets.xcassets/AppIcon.appiconset/Contents.json` | the icon set: one `universal` `ios` image of `1024x1024` |
| `<out>/<name>.assets/Assets.xcassets/AppIcon.appiconset/icon-1024.png` | a copy of `--icon` |
| `<out>/<name>.assets/partial.plist` | the `Info.plist` keys `actool` wrote |
| `<name>.app/Assets.car` | the compiled catalog |
| `<name>.app/AppIcon60x60@2x.png`, `<name>.app/AppIcon76x76@2x~ipad.png` | the icons iOS reads without the catalog |

Every key of `partial.plist` goes into the bundle's `Info.plist`: `CFBundleIcons` and
`CFBundleIcons~ipad`, each a `CFBundlePrimaryIcon` with its `CFBundleIconFiles` and the
`CFBundleIconName` `AppIcon`. A bundle staged again without `--icon` has none of these.

### The privacy manifest

The App Store wants an app to say, in a `PrivacyInfo.xcprivacy` file in its bundle, whether
it tracks, what data it collects and why it calls each API on Apple's required-reason list.
`scripts/ios-app` copies one to `<name>.app/PrivacyInfo.xcprivacy`: Rune's own,
`ios/RuneView/Sources/RuneView/PrivacyInfo.xcprivacy`, unless `--privacy FILE` names the
app's. Rune's is the only copy in the repository: the `RuneView` package ships it as a
resource, and `scripts/ios-lib` copies it to `<out>/PrivacyInfo.xcprivacy`.

Rune tracks nothing and collects nothing, so `NSPrivacyTracking` is false and
`NSPrivacyTrackingDomains` and `NSPrivacyCollectedDataTypes` are empty. Under
`NSPrivacyAccessedAPITypes` it declares the two listed APIs a Rune app's binary links:

- **File timestamp** (`NSPrivacyAccessedAPICategoryFileTimestamp`, reason `C617.1`), for
  `stat` and `fstat`. Rune reads no timestamp. The calls are:
  - `prefs.json` in the app's Application Support folder: a read, and creating the folder;
  - the files an app names to `ImageCache::file`, a `file:` image URL or `App::font_url`:
    reads, which ask the file's size first;
  - the system's font files: existence, is-directory and size checks made through fontique
    and memmap2 when a system font is loaded, and one existence check of Rune's own on the
    font files beside the system UI font. These files are outside the app's container, so
    this use fits none of Apple's reasons exactly; `C617.1` is the closest.
- **System boot time** (`NSPrivacyAccessedAPICategorySystemBootTime`, reason `35F9.1`), for
  `mach_absolute_time`, linked from wgpu-hal's Metal presentation timestamp.

Rune uses none of the other listed APIs: not disk space, not user defaults, not the active
keyboards.

Rune itself reads nothing the user picks in a file picker. `Tx::open_file` hands the app a
copy inside its container; `Tx::open_folder` and `Tx::save_file` hand it a path under a
security-scoped folder. An app that reads files inside a picked folder declares `3B52.1` for
the file timestamp category itself.

That and everything else an app does on its own (user defaults, a crate that asks for free
disk space, data it collects, tracking) goes in the app's own manifest: start from a copy of
Rune's file, add the app's entries and pass it with `--privacy`. The script refuses a file
that is not a property list or lacks one of the four top-level keys; what the entries say is
the app's to get right. To see which listed APIs an app's own binary links, list the symbols
it imports and look them up in Apple's list
([Describing use of required reason API](https://developer.apple.com/documentation/bundleresources/describing-use-of-required-reason-api)):

```sh
cargo build -p my-app --target aarch64-apple-ios --release
nm -u target/aarch64-apple-ios/release/my-app | \
    grep -E 'stat|getattrlist|utimes|mach_absolute_time|statfs|statvfs|NSUserDefaults|activeInputModes'
```

### A build an app could upload

`--distribution` makes the files the App Store takes and installs nothing:

```sh
cargo build -p hello --target aarch64-apple-ios --release
scripts/ios-app --name Hello --id com.example.hello --bin target/aarch64-apple-ios/release/hello \
    --version 1.0.0 --build 1 --icon path/to/icon.png \
    --distribution --profile path/to/app-store.mobileprovision
```

It needs `--profile`, an App Store provisioning profile, and `--icon`, since the App Store
takes no app without one, and takes neither `--device` nor `--simulator`. Before staging
anything it refuses:

- a profile `security cms` can't read, or one that grants no entitlements;
- a profile that lists devices (development or ad hoc), provisions all devices (enterprise),
  lets a debugger attach (`get-task-allow`), has expired, or whose app identifier is neither
  `--id` under the profile's team nor a wildcard that covers it;
- an `IOS_APP_SIGN` that names a development identity (`Apple Development`,
  `iPhone Developer`);
- a binary built for the simulator.

Every run of `ios-app`, with `--distribution` or without, first removes
`<out>/<name>.ipa` and `<out>/<name>.app.dSYM` from an earlier run, so a failed build leaves
no old archive beside a new bundle.

With `--distribution` the script then does these, in order:

1. `xcrun dsymutil` writes `<out>/<name>.app.dSYM` from the binary, and what it warns of is
   printed. With this workspace's release profile the dSYM holds symbol names only, and
   dsymutil warns `no debug symbols in executable`; "Line numbers and build paths" below
   adds files and line numbers to it. Keep the dSYM: nothing uploads it, and it is what
   Xcode's Organizer, `atos` and `symbolicatecrash` need to turn a crash log's addresses
   into names.
2. `xcrun strip -S -x` takes the debugging entries, which name the build's object files by
   their full paths, and the local symbols out of the bundle's binary. It can't take the
   source paths Rust keeps as strings for its panic messages ("Line numbers and build
   paths" below).
3. The bundle is signed with the keychain's `Apple Distribution` identity (`IOS_APP_SIGN`
   names another) and the profile's entitlements, its app identifier narrowed to `--id`, and
   the profile is embedded as `<name>.app/embedded.mobileprovision`. Everything else the
   profile grants is signed as it is, wildcards included, which is also how a device build
   is signed ("Run on a device" above).
4. `codesign --verify --deep --strict` checks the signature and `plutil -lint` the
   `Info.plist`.
5. `<out>/<name>.ipa` is written: a zip of `Payload/<name>.app` and nothing else, with an
   entry for each folder, each file with its mode and each symbolic link as a link, without
   `.DS_Store`, `._` files or `__MACOSX`.

The script prints the bundle's, the dSYM's and the .ipa's paths. With `IOS_APP_SIGN=-` the
bundle is signed ad hoc and `--profile` and `--icon` may be left out: that proves the steps
on a Mac with no distribution identity, and the result can't be uploaded.

The script never uploads, and no upload has been made of an .ipa it wrote. Upload with the
Transporter app, or with `xcrun altool --upload-app -f target/ios/Hello.ipa -t ios` and an
App Store Connect API key: `xcrun altool --help` lists the flags that name the key.

Whether the app uses encryption that needs export paperwork is the app's own call, so the
script doesn't set `ITSAppUsesNonExemptEncryption`.

### Line numbers and build paths

What follows was seen with a release device build of `web-hello`, Xcode 27.2 and Rust
1.99.1.

**Build paths.** The plain release binary held 594 strings with the build machine's home
folder in them, every one the path of a dependency's source file under
`.cargo/registry/src/`, kept for panic messages. The workspace's own paths are relative and
the standard library's are `/rustc/<hash>/...`. `strip` removed none of them. Built with

```sh
RUSTFLAGS="--remap-path-prefix=$HOME/.cargo=/cargo --remap-path-prefix=$PWD=/rune" \
    cargo build --release --target aarch64-apple-ios -p web-hello
```

the binary held none (they read `/cargo/registry/src/.../smallvec-1.16.3/src/lib.rs`) and
was the same size. `RUSTFLAGS` in the environment replaces the `build.rustflags` and the
target's `rustflags` a config file would apply to the target; none apply to iOS in this
workspace. A `.cargo/config.toml` can't expand `$HOME`, so set the variable in the shell or
the CI step that builds. Cargo's `trim-paths` is not stable in Rust 1.99.1.

**Line numbers.** With `CARGO_PROFILE_RELEASE_DEBUG=line-tables-only` as well, the dSYM
went from names only (no compile units, 4.5 MB) to 2,505 compile units (106 MB), and `atos`
turned an address into `open.rs:12` (with `-fullPath`,
`/rune/crates/rune/src/playground/open.rs:12`). The unstripped binary grew from 22.9 MB to
25.0 MB, the stripped one stayed at 20.1 MB, and the .ipa was 9.4 MB.

The debug information stays in the build's object files until `dsymutil` collects it, so
run `ios-app --distribution` while the target directory still holds them. The unstripped
binary also lists those object files by their absolute paths, and the strip step left none
of them. One C file, blake3's NEON source, kept the home folder in the dSYM until
`CFLAGS="-ffile-prefix-map=$HOME/.cargo=/cargo"` was set too; with it the dSYM held none.
Don't set `split-debuginfo = "packed"`: cargo then writes a dSYM of its own and deletes the
object files, and the script's `dsymutil` finds little (it gave three
`unable to open object file` warnings).

The whole recipe:

```sh
RUSTFLAGS="--remap-path-prefix=$HOME/.cargo=/cargo --remap-path-prefix=$PWD=/rune" \
CFLAGS="-ffile-prefix-map=$HOME/.cargo=/cargo" \
CARGO_PROFILE_RELEASE_DEBUG=line-tables-only \
    cargo build --release --target aarch64-apple-ios -p web-hello
```

## What the `Info.plist` needs

`scripts/ios-app` writes these, with the bundle's name, identifier, executable and versions
("Versions and names" above), and `CFBundlePackageType` `APPL`. An app with its own bundle
needs the same:

- `UIApplicationSceneManifest` with `UIApplicationSupportsMultipleScenes` false and no
  configurations: Rune supplies the scene's in code, and shows one window.
- `UILaunchScreen` (an empty dictionary will do), or iOS letterboxes the app.
- `CADisableMinimumFrameDurationOnPhone` set to true, or an iPhone holds the app at 60 Hz.
- `MinimumOSVersion` 16.0 or later (`--min-os`), the same as the binary was built for
  ("The deployment target" above).
- `UIDeviceFamily` `[1, 2]` (iPhone and iPad): without the 2 an iPad runs the app as an
  iPhone app, in a phone-sized frame.
- `UISupportedInterfaceOrientations` with portrait and both landscapes, for an iPhone.
- `UISupportedInterfaceOrientations~ipad` with all four orientations, or an iPad runs the app
  full screen only, never in Split View or a Stage Manager window.
- `CFBundleURLTypes` with the app's schemes, when it takes links (`--url-scheme`).
- `NSMicrophoneUsageDescription`, and `UIBackgroundModes` with `audio`, for voice calls
  ("Voice calls" above).
- `CFBundleIcons` and `CFBundleIcons~ipad` naming the icon in the bundle's `Assets.car`, when
  it has an icon (`--icon`; "The icon" above).

The script also writes the keys Xcode writes to say what the bundle runs on
and what built it. An app with its own bundle gets them from Xcode. The script reads them on
the Mac it runs on, before it stages anything:

| Key | Value | From |
| --- | --- | --- |
| `CFBundleSupportedPlatforms` | `["iPhoneOS"]`, or `["iPhoneSimulator"]` | the platform the binary was built for |
| `DTPlatformName` | `iphoneos`, or `iphonesimulator` | the same |
| `DTPlatformVersion` | the SDK's version, as `27.2` | `xcrun --sdk <sdk> --show-sdk-version` |
| `DTSDKName` | the SDK's name and version, as `iphoneos27.2` | the same |
| `DTSDKBuild`, `DTPlatformBuild` | the SDK's build, as `24B8118` | `xcrun --sdk <sdk> --show-sdk-build-version` |
| `DTXcode` | Xcode's version as four digits: `2720` for 27.2 | `xcodebuild -version` |
| `DTXcodeBuild` | Xcode's build, as `27B52` | the same |
| `BuildMachineOSBuild` | the Mac's macOS build | `sw_vers -buildVersion` |
| `DTCompiler` | `com.apple.compilers.llvm.clang.1_0` | fixed |
| `LSRequiresIPhoneOS` | true | fixed |
| `UIRequiredDeviceCapabilities` | `["arm64"]`, in a device build only | the platform the binary was built for |
| `CFBundleInfoDictionaryVersion` | `6.0` | fixed |
| `CFBundleDevelopmentRegion` | `en` | fixed |

The keys and their values are the ones Xcode writes: they were checked against an app Xcode
built on the same Mac, for a device build and for a simulator build. No upload has been made
with a bundle from this script.

Beside the `Info.plist` and the binary the bundle holds `PrivacyInfo.xcprivacy`, which the
App Store reads ("The privacy manifest" above), the icon's files when it has one, and
`embedded.mobileprovision` when it was signed with a profile.
