import type { ReactNode } from "react"
import { withBase } from "./base"
import { Code } from "./Code"
import { Pager, Sample } from "./pages"
import { REPO } from "./routes"
import { PageHead, SectionHead } from "./system/Frame"

const GUIDE = `${REPO}/blob/dev/IOS.md`

const header = `void *my_screen(void);
void my_open_url(const char *url);`

const deploymentTarget = `[env]
IPHONEOS_DEPLOYMENT_TARGET = "16.0"`

const linkedApis = `cargo build -p my-app --target aarch64-apple-ios --release
nm -u target/aarch64-apple-ios/release/my-app | \\
    grep -E 'stat|getattrlist|utimes|mach_absolute_time|statfs|statvfs|NSUserDefaults|activeInputModes'`

const distribution = `cargo build -p hello --target aarch64-apple-ios --release
scripts/ios-app --name Hello --id com.example.hello --bin target/aarch64-apple-ios/release/hello \\
    --version 1.0.0 --build 1 --icon path/to/icon.png \\
    --distribution --profile path/to/app-store.mobileprovision`

const lineTables = `RUSTFLAGS="--remap-path-prefix=$HOME/.cargo=/cargo --remap-path-prefix=$PWD=/rune" \\
CFLAGS="-ffile-prefix-map=$HOME/.cargo=/cargo" \\
CARGO_PROFILE_RELEASE_DEBUG=line-tables-only \\
    cargo build --release --target aarch64-apple-ios -p web-hello`

const association =`{"applinks": {"details": [{"appIDs": ["TEAMID.com.example.hello"], "components": [{"/": "/item/*"}]}]}}`

const hooks: { system: string; app: ReactNode }[] = [
  {
    system: "The scene enters the foreground, or the view joins a window in front",
    app: (
      <>
        <code>App::on_visibility(true)</code>; <code>AppHandle::is_visible</code> is true
      </>
    ),
  },
  {
    system: "The scene enters the background, or the view leaves its window",
    app: (
      <>
        <code>App::on_visibility(false)</code>; <code>AppHandle::is_visible</code> is false. Then the view rests,
        and <code>App::on_rest</code> runs
      </>
    ),
  },
  { system: "The scene becomes active", app: <code>App::on_focus(true)</code> },
  {
    system: "The scene resigns active: Control Center, the app switcher, a call",
    app: <code>App::on_focus(false)</code>,
  },
  {
    system: "The app will terminate",
    app: (
      <>
        <code>App::on_focus(false)</code> and <code>App::on_visibility(false)</code> if it had not heard them, then{" "}
        <code>App::on_quit</code>, once. No rest
      </>
    ),
  },
]

export function IosView() {
  return (
    <>
      <PageHead eyebrow={<span>iOS</span>} title="iOS">
        <p>
          Rune's iOS target is experimental. It draws one <code>UIView</code> on a <code>CAMetalLayer</code> with the
          same Rust views, layout, text and shaders as the desktop. A display link draws at up to 120 Hz while
          something moves and stops when nothing does, so an idle app uses no CPU or GPU.
        </p>
        <p>
          Status: scroll to top, colour emoji, spell checking and the undo gestures have been shown on the iOS
          simulator only, and none of them is verified on a device yet. <code>scripts/ios-app</code> makes a build an
          app could upload and never uploads it, and no upload has been made with one.
        </p>
        <p>
          This page is the short guide. <a href={GUIDE}>IOS.md</a> is the reference, with every flag, caveat and
          measurement.
        </p>
      </PageHead>

      <section className="doc-section">
        <SectionHead id="works" title="What works">
          <p>
            The same app code as on a desktop. What iOS adds reaches the app through the hooks and values it already
            uses.
          </p>
        </SectionHead>
        <div className="prose">
          <ul>
            <li>
              <strong>Drawing.</strong> Light and dark appearance, the screen's own scale, up to 120 Hz, and nothing
              drawn while idle or in the background.
            </li>
            <li>
              <strong>Touch and pointer.</strong> One finger taps, drags and pans with momentum; two fingers pinch (
              <code>on_magnify</code>) and turn (<code>on_rotate</code>); a long press is the secondary press, so it
              opens what a right-click opens on a desktop. A mouse or trackpad on an iPad hovers, clicks, right-clicks
              and scrolls. A tap on the status bar glides the app's main list to its top (
              <a href={`${GUIDE}#scroll-to-top`}>Scroll to top</a>).
            </li>
            <li>
              <strong>Safe area.</strong> <code>Cx::safe_area</code> holds what the status bar, the home indicator and
              the soft keyboard take: pad a full-screen root by it.
            </li>
            <li>
              <strong>Text input.</strong> The soft keyboard shows while a text field has the focus. Typing, delete,
              return, predictions, autocorrection, dictation and marked text edit the field, and the space bar's
              trackpad moves the caret. A secure field shows the keyboard none of its text.
            </li>
            <li>
              <strong>Keyboard traits.</strong> <code>El::input_mode</code> picks the keyboard (number, decimal and
              phone pads, email, URL, search), <code>El::autocomplete</code> the content type, and{" "}
              <code>El::autocapitalize</code>, <code>El::autocorrect</code> and <code>El::enter_key_hint</code> the
              capitalization, autocorrection and Return key. With a content type the system may offer a saved
              password, a strong new one or a code from a message; whether it does is its decision.
            </li>
            <li>
              <strong>Spelling and undo.</strong> A field with <code>El::spellcheck(true)</code> marks misspelled
              words and leads its long-press menu with suggestions (
              <a href={`${GUIDE}#spell-checking`}>Spell checking</a>). A shake and the three-finger gestures undo and
              redo the focused field's own history (<a href={`${GUIDE}#undo-gestures`}>Undo gestures</a>).
            </li>
            <li>
              <strong>Fonts and emoji.</strong> Text draws in the fonts the app bundles, Inter by default.{" "}
              <code>Family::Named("System Font")</code> is San Francisco and <code>Family::SystemMono</code> SF Mono.
              A character the app's fonts lack draws in the font CoreText falls back to, and emoji draw in colour.
            </li>
            <li>
              <strong>Accent.</strong> <code>Cx::accent</code> is the view's <code>tintColor</code>, which is where a
              UIKit host's <code>tintColor</code> and a SwiftUI host's <code>.tint()</code> land; with none set it is
              iOS's system blue. <code>Cx::focus_color</code> is that colour at half its alpha.
            </li>
            <li>
              <strong>Hardware keyboard and menus.</strong> Arrows, Return, Escape, Tab and the function keys with
              their modifiers, key repeat, and shortcuts with ⌘ as on a Mac. In an app <code>rune::run</code> started,{" "}
              <code>App::menu_item</code> items join an iPad's menu bar. Copy, cut and paste use the system
              pasteboard, and a <code>Tx::popup_menu</code> menu shows as the system's edit menu.
            </li>
            <li>
              <strong>Accessibility.</strong> VoiceOver, Switch Control and Voice Control read the same accessibility
              tree the desktop platforms do. Reduce Motion is followed, and <code>Cx::system_text_scale</code> is the
              Dynamic Type size.
            </li>
            <li>
              <strong>Handles and memory.</strong> <code>AppHandle::update</code> and <code>wake</code> reach the app
              from a background thread, and <code>App::on_ready</code> runs once the first frame is on screen. On a
              memory warning the view rests at once: it runs <code>App::on_rest</code> and frees the caches its last
              frame didn't draw from.
            </li>
            <li>
              <strong>Links and files.</strong> <code>Tx::open_url</code> opens a link in the app that owns its
              scheme. <code>Tx::open_file</code> shows the system's document picker and answers with a copy of the
              file; <code>Tx::open_folder</code> answers with the folder picked, and <code>Tx::save_file</code> asks
              for a folder and answers with the dialog's file name inside it. After either, the app may read and
              write in the picked folder until it quits, and not after a relaunch: ask again then. The picker opens
              in the options' <code>directory</code> when the system can show it, shows no <code>title</code>, and
              a save doesn't ask before a file of that name is replaced. An iOS app has no Downloads folder, so <code>AppDirs::downloads</code> is <code>None</code>.
            </li>
          </ul>
        </div>
      </section>

      <section className="doc-section">
        <SectionHead id="missing" title="What doesn't yet">
          <p>Five gaps to plan around.</p>
        </SectionHead>
        <div className="prose">
          <ul>
            <li>The system's selection handles and loupe: a field draws its own selection.</li>
            <li>
              An app's menu items in the menu bar of a UIKit or SwiftUI app that embeds a Rune view. On an iPad only a{" "}
              <code>keys_first</code> item's shortcut runs there; on an iPhone every item's does.
            </li>
            <li>
              More than one surface an app. A <code>RuneView</code> is one view, so other windows and overlays never
              open, and requests of a window (<code>show</code>, <code>hide</code>, <code>close</code>,{" "}
              <code>quit</code> and the rest) are dropped. The first dropped request of each kind prints one{" "}
              <code>rune: ... ignored: ...</code> line on stderr.
            </li>
            <li>
              Chinese in the system's own font. iOS's Chinese font is PingFang, whose glyphs are in a table Rune has
              no reader for. An app with Chinese text bundles a font for it (<code>AppSpec::fonts</code>).
            </li>
            <li>
              Intel Macs' simulators: <code>scripts/ios-lib</code> builds for devices and Apple silicon simulators.
            </li>
          </ul>
        </div>
      </section>

      <section className="doc-section">
        <SectionHead id="simulator" title="Build and run in a simulator">
          <p>
            Install the targets and Xcode with an iOS simulator runtime, then build any Rune binary and hand it to{" "}
            <a href={`${REPO}/blob/dev/scripts/ios-app`}>scripts/ios-app</a>. No Xcode project is involved.
          </p>
        </SectionHead>
        <Code id="ios-simulator" />
        <div className="prose prose-after">
          <p>
            The script stages <code>target/ios/Hello.app</code> (the binary, an <code>Info.plist</code>, a privacy
            manifest and, with <code>--icon</code>, the icon's files), signs it ad hoc, installs it on the booted simulator and launches it. With no simulator booted the install fails
            with <code>No devices are booted</code>: <code>--simulator NAME_OR_UDID</code> picks one and boots it
            first. <code>--no-launch</code> and <code>--no-install</code> stop earlier, and arguments after{" "}
            <code>--</code> go to the app.
          </p>
          <p>
            An app can die as it starts in two ways, and the script fails on both. The app launches and is gone a
            moment later: <code>--wait SECONDS</code> after the launch (3 unless told; <code>--wait 0</code> skips
            this check) the launched process must still be running, or the script exits non-zero and says where the
            crash log is. Or the launch itself fails, because the app died before it had a process: the script exits
            non-zero and says where the crash log is, with what the tool said. A check that passes proves that much
            and no more: on a simulator that has just booted the app can still be on its way to the screen then.
          </p>
        </div>
      </section>

      <section className="doc-section">
        <SectionHead id="device" title="Run on a device">
          <p>
            A device runs only what a development provisioning profile covers: one that lists the device and whose app
            identifier matches <code>--id</code>. A wildcard profile matches any.
          </p>
        </SectionHead>
        <Code id="ios-device" />
        <div className="prose prose-after">
          <p>
            The script embeds the profile and signs with the profile's entitlements and the keychain's{" "}
            <code>Apple Development</code> identity; set <code>IOS_APP_SIGN</code> to name another.{" "}
            <code>xcrun devicectl list devices</code> lists paired devices.
          </p>
          <p>
            <code>--wait</code> checks a device's launch too. The device keeps its own crash logs, so for an app that
            died the script prints the commands that list them and copy one to the Mac:
          </p>
        </div>
        <Code id="ios-crash-logs" />
      </section>

      <section className="doc-section">
        <SectionHead id="entry" title="The app's entry point">
          <p>
            The usual one. On iOS <code>rune::run</code> and <code>App::run</code> start UIKit, make the window and
            fill it with the app, and never return.
          </p>
        </SectionHead>
        <Code id="ios-entry" />
        <div className="prose prose-after">
          <p>
            <code>AppSpec</code>'s size is ignored: the app is the size of the screen. Move the app's own desktop-only
            crates (a file dialog crate, hot reload, the system tray) behind target conditions. Rune's file dialogs
            need none.
          </p>
        </div>
      </section>

      <section className="doc-section">
        <SectionHead id="embed" title="A Rune view in a UIKit or SwiftUI app">
          <p>
            <code>rune::ios::view(app)</code> returns a retained <code>UIView *</code> that draws <code>app</code> and
            fills whatever it is put in. Export a function that returns it from the app's library, and declare it in a
            C header.
          </p>
        </SectionHead>
        <div className="code-stack">
          <Code id="ios-view" />
          <Sample lang="C">{header}</Sample>
        </div>
        <div className="prose prose-after">
          <p>
            <a href={`${REPO}/blob/dev/scripts/ios-lib`}>scripts/ios-lib</a> builds the library for devices and Apple
            silicon simulators and writes an XCFramework that holds both, the header and a module map. The module map
            links the system frameworks Rune uses, so the host app adds no linker flags.
          </p>
        </div>
        <Code id="ios-lib" />
        <div className="prose prose-after">
          <p>
            In Xcode, add <code>target/ios/MyApp.xcframework</code> to the app target's frameworks and add the{" "}
            <a href={`${REPO}/tree/dev/ios/RuneView`}>ios/RuneView</a> Swift package, then:
          </p>
        </div>
        <Code id="ios-swiftui" />
        <div className="prose prose-after">
          <p>
            <code>RuneView</code> is a <code>UIViewRepresentable</code>; a UIKit app takes the pointer itself with{" "}
            <code>Unmanaged&lt;UIView&gt;.fromOpaque(my_screen()).takeRetainedValue()</code>. Call the function on the
            main thread: it returns null anywhere else. The view reports the safe area of where it sits, so one that
            doesn't reach the screen's edges reads zero.{" "}
            <a href={`${REPO}/tree/dev/examples/web-hello`}>examples/web-hello</a> exports{" "}
            <code>web_hello_view</code> this way.
          </p>
        </div>
      </section>

      <section className="doc-section">
        <SectionHead id="links" title="Links into the app">
          <p>
            <code>App::on_open_url</code> gets the links iOS opens with the app: its own schemes' and universal links,
            the one that launched it among them. The string comes from any app or web page, so parse and validate it
            before acting on it.
          </p>
        </SectionHead>
        <div className="prose">
          <p>
            A link that launches the app runs before the first frame; one that reaches the running app wakes it and
            redraws. <code>scripts/ios-app --url-scheme myapp</code> declares a scheme of the app's own in the bundle's{" "}
            <code>Info.plist</code>; repeat the flag for more than one.
          </p>
        </div>
        <Code id="ios-url-scheme" />
        <div className="prose prose-after">
          <p>
            Universal links (<code>https://example.com/item/7</code> opens the app instead of Safari) arrive at the
            same hook with the web URL. Rune delivers them; the app and its web server say which links they are, and{" "}
            <code>scripts/ios-app</code> sets none of this up:
          </p>
          <ul>
            <li>
              Give the app's identifier the Associated Domains capability, and make a provisioning profile that
              carries it.
            </li>
            <li>
              Sign the app with the <code>com.apple.developer.associated-domains</code> entitlement listing each
              domain as <code>applinks:example.com</code>.
            </li>
            <li>
              Serve <code>https://example.com/.well-known/apple-app-site-association</code> as JSON, with no file
              extension and no redirect, naming the app and the paths it takes.
            </li>
          </ul>
        </div>
        <Sample lang="JSON">{association}</Sample>
        <div className="prose prose-after">
          <p>
            A Rune view in another app gets no links from iOS: that app's own scene delegate is handed them. It passes
            each on with <code>rune::ios::open_url(url)</code>, which runs <code>on_open_url</code> exactly as above.
            Export a function for it beside the one that makes the view. A SwiftUI app calls it from{" "}
            <code>onOpenURL</code>, which gets both kinds of link; a UIKit app calls it from its scene delegate.
          </p>
        </div>
        <div className="code-stack">
          <Code id="ios-open-url" />
          <Code id="ios-on-open-url" />
        </div>
      </section>

      <section className="doc-section">
        <SectionHead id="lifecycle" title="Lifecycle hooks">
          <p>
            A <code>RuneView</code> follows the scene of the window it is in, so on an iPad each of an app's scenes
            tells its own view, and a view inside a UIKit or SwiftUI app hears the same as one <code>rune::run</code>{" "}
            made.
          </p>
        </SectionHead>
        <div className="table-wrap">
          <table className="decisions">
            <thead>
              <tr>
                <th scope="col">The system says</th>
                <th scope="col">The app hears</th>
              </tr>
            </thead>
            <tbody>
              {hooks.map(h => (
                <tr key={h.system}>
                  <th scope="row" style={{ width: "42%" }}>
                    {h.system}
                  </th>
                  <td data-label="The app hears" style={{ width: "auto" }}>
                    {h.app}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <div className="prose prose-after">
          <p>
            Each hook hears a value once. Coming forward the app is visible before it has the focus, and going away it
            loses the focus before it hides. When the scene stops being active what was held ends: a drag under a
            finger hears its cancelled <code>Phase::Up</code>, then every key held on a hardware keyboard is released.
          </p>
          <p>
            A view that goes out of sight rests, as a hidden desktop window does: Rune's caches go back to what the
            screen on show uses, <code>App::on_rest</code> runs for the app's own, and the renderer gives back what
            it keeps between frames. That happens inside the system's notification, so keep <code>on_rest</code>{" "}
            quick, and nothing is submitted to the GPU there. A view on screen and idle does not rest on iOS, as a
            desktop window does after 10 s without a frame.
          </p>
          <p>
            iOS announces termination only to an app that is running when it ends, never to a suspended one, so{" "}
            <code>on_quit</code> is a last chance: save when <code>on_visibility</code> says false.{" "}
            <code>App::on_close</code> and <code>App::on_reopen</code> are never called, since a view is no window to
            close and iOS reopens no running app.
          </p>
        </div>
      </section>

      <section className="doc-section">
        <SectionHead id="ship" title="Ship an app">
          <p>
            A bundle to ship is built for the deployment target it names, has its own versions, an icon and a privacy
            manifest that covers the app, and is made with <code>--distribution</code>.
          </p>
        </SectionHead>
        <div className="prose">
          <h3>One deployment target</h3>
          <p>
            Rune runs on iOS 16 and later: its Metal backend writes MSL 3.0. The binary and the bundle each say the
            oldest iOS the app runs on, and they have to say the same. The binary's comes from{" "}
            <code>IPHONEOS_DEPLOYMENT_TARGET</code> when it is built. Rune's own <code>.cargo/config.toml</code> sets
            it, but a workspace config is not inherited by an app that depends on Rune, so the app sets it in its own:
          </p>
        </div>
        <Sample lang="TOML">{deploymentTarget}</Sample>
        <div className="prose prose-after">
          <p>
            The bundle's is <code>MinimumOSVersion</code>, which <code>--min-os X.Y</code> writes (16.0 unless told;
            below 16.0 is refused). Before signing, the script reads what the staged binary was built for and stops
            when it is not <code>--min-os</code>. <code>scripts/ios-lib</code> builds with 16.0 unless the caller set
            it.
          </p>
          <h3>Versions and names</h3>
          <p>
            <code>--display-name</code> is the name under the icon (<code>--name</code> unless told),{" "}
            <code>--version</code> the version users see (<code>0.0.1</code>) and <code>--build</code> the build's own
            number (<code>1</code>). A version and a build number are each one to three whole numbers joined by dots,
            18 characters at most; anything else is refused, since the App Store refuses it. The App Store also wants
            each upload of one version to carry a higher build number than the last, which is the app's to keep count
            of.
          </p>
          <h3>The icon</h3>
          <p>
            <code>--icon PNG</code> takes one image, 1024×1024 and opaque, and the system makes every size from it.
            The script refuses a file that is not a PNG, one of another size, and one with an alpha channel or a
            transparency chunk (<code>tRNS</code>), since the App Store rejects an icon that can be see-through. It compiles an asset catalog into the bundle with{" "}
            <code>xcrun actool</code>. Without <code>--icon</code> the bundle has no icon and iOS shows a blank one.
          </p>
          <h3>The privacy manifest</h3>
          <p>
            The App Store wants an app to say, in a <code>PrivacyInfo.xcprivacy</code> file in its bundle, whether it
            tracks, what data it collects and why it calls each API on Apple's required-reason list.{" "}
            <code>scripts/ios-app</code> copies{" "}
            <a href={`${REPO}/blob/dev/ios/RuneView/Sources/RuneView/PrivacyInfo.xcprivacy`}>Rune's own</a> into the
            bundle unless <code>--privacy FILE</code> names the app's. Rune tracks nothing and collects nothing, and
            declares the two listed APIs a Rune app's binary links:
          </p>
          <ul>
            <li>
              <strong>File timestamp</strong> (reason <code>C617.1</code>), for <code>stat</code> and{" "}
              <code>fstat</code> on <code>prefs.json</code>, the files an app names to Rune and the system's font
              files. Rune reads no timestamp.
            </li>
            <li>
              <strong>System boot time</strong> (reason <code>35F9.1</code>), for <code>mach_absolute_time</code>,
              linked from wgpu-hal's Metal presentation timestamp.
            </li>
          </ul>
          <p>
            Everything an app does on its own (user defaults, a crate that asks for free disk space, data it collects,
            tracking, reading files inside a picked folder) goes in the app's own manifest: start from a copy of
            Rune's file, add the app's entries and pass it with <code>--privacy</code>. The script refuses a file that
            is not a property list or lacks one of the four top-level keys; what the entries say is the app's to get
            right. To see which listed APIs an app's own binary links, list the symbols it imports:
          </p>
        </div>
        <Sample lang="Shell">{linkedApis}</Sample>
        <div className="prose prose-after">
          <h3>A build an app could upload</h3>
          <p>
            <code>--distribution</code> makes the files the App Store takes and installs nothing. It needs{" "}
            <code>--profile</code>, an App Store provisioning profile, and <code>--icon</code>, since the App Store
            takes no app without one. Before staging anything it refuses a profile that lists devices (development or
            ad hoc), provisions all devices, lets a debugger attach, has expired or does not cover <code>--id</code>;
            an <code>IOS_APP_SIGN</code> that names a development identity; and a binary built for the simulator.
          </p>
        </div>
        <Sample lang="Shell">{distribution}</Sample>
        <div className="prose prose-after">
          <ul>
            <li>
              <code>xcrun dsymutil</code> writes <code>&lt;name&gt;.app.dSYM</code> from the binary. With Rune's
              release profile it holds symbol names only; the build below adds files and line numbers. Keep it:
              nothing uploads it, and it is what turns a crash log's addresses into names.
            </li>
            <li>
              <code>xcrun strip -S -x</code> takes the debugging entries and the local symbols out of the bundle's
              binary. It can't take the source paths Rust keeps as strings for its panic messages.
            </li>
            <li>
              The bundle is signed with the keychain's <code>Apple Distribution</code> identity and the profile's
              entitlements, and the profile is embedded.
            </li>
            <li>
              <code>codesign --verify --deep --strict</code> checks the signature and <code>plutil -lint</code> the{" "}
              <code>Info.plist</code>.
            </li>
            <li>
              <code>&lt;name&gt;.ipa</code> is written: a zip of <code>Payload/&lt;name&gt;.app</code> and nothing
              else.
            </li>
          </ul>
          <p>
            With <code>IOS_APP_SIGN=-</code> the bundle is signed ad hoc and <code>--profile</code> and{" "}
            <code>--icon</code> may be left out: that proves the steps on a Mac with no distribution identity, and the
            result can't be uploaded.
          </p>
          <p>
            The script never uploads, and no upload has been made of an .ipa it wrote. Upload with the Transporter app,
            or with <code>xcrun altool --upload-app -f target/ios/Hello.ipa -t ios</code> and an App Store Connect API
            key: <code>xcrun altool --help</code> lists the flags that name the key. Whether the app uses encryption
            that needs export paperwork is the app's own call, so the script doesn't set{" "}
            <code>ITSAppUsesNonExemptEncryption</code>.
          </p>
          <h3>Line numbers and build paths</h3>
          <p>
            A plain release build's dSYM has names and no lines, and its binary holds the build machine's home folder
            in the source paths of its dependencies, which <code>strip</code> doesn't remove. This build gives the
            dSYM files and line numbers and leaves no home folder in the binary or the dSYM, as seen with{" "}
            <code>web-hello</code>, Xcode 27.2 and Rust 1.99.1:
          </p>
        </div>
        <Sample lang="Shell">{lineTables}</Sample>
        <div className="prose prose-after">
          <p>
            The stripped binary stayed the same size, and the dSYM grew from 4.5 MB to 106 MB. The debug information
            stays in the build's object files until <code>dsymutil</code> collects it, so run{" "}
            <code>ios-app --distribution</code> while the target directory still holds them, and don't set{" "}
            <code>split-debuginfo = "packed"</code>, which deletes them. <code>RUSTFLAGS</code> in the environment
            replaces the rustflags a config file sets for the target.
          </p>
        </div>
      </section>

      <section className="doc-section">
        <SectionHead id="plist" title="What the Info.plist needs">
          <p>
            <code>scripts/ios-app</code> writes these, with the bundle's name, identifier, executable and versions, and{" "}
            <code>CFBundlePackageType</code> <code>APPL</code>. An app with its own bundle needs the same.
          </p>
        </SectionHead>
        <div className="prose">
          <ul>
            <li>
              <code>UIApplicationSceneManifest</code> with <code>UIApplicationSupportsMultipleScenes</code> false and
              no configurations: Rune supplies the scene's in code, and shows one window.
            </li>
            <li>
              <code>UILaunchScreen</code> (an empty dictionary will do), or iOS letterboxes the app.
            </li>
            <li>
              <code>CADisableMinimumFrameDurationOnPhone</code> set to true, or an iPhone holds the app at 60 Hz.
            </li>
            <li>
              <code>MinimumOSVersion</code> 16.0 or later, the same as the binary was built for.
            </li>
            <li>
              <code>UIDeviceFamily</code> <code>[1, 2]</code>: without the 2 an iPad runs the app as an iPhone app, in
              a phone-sized frame.
            </li>
            <li>
              <code>UISupportedInterfaceOrientations</code> with portrait and both landscapes for an iPhone, and{" "}
              <code>UISupportedInterfaceOrientations~ipad</code> with all four, or an iPad runs the app full screen
              only.
            </li>
            <li>
              <code>CFBundleURLTypes</code> with the app's schemes, when it takes links.
            </li>
            <li>
              <code>CFBundleIcons</code> and <code>CFBundleIcons~ipad</code> naming the icon in the bundle's{" "}
              <code>Assets.car</code>, when it has an icon.
            </li>
          </ul>
          <p>
            The script also writes the keys Xcode writes to say what the bundle runs on and what built
            it: <code>CFBundleSupportedPlatforms</code>, the <code>DT</code> keys for the platform, SDK and Xcode,{" "}
            <code>BuildMachineOSBuild</code>, <code>LSRequiresIPhoneOS</code>,{" "}
            <code>UIRequiredDeviceCapabilities</code> in a device build, and a few more. An app with its own bundle gets them from Xcode.
            The keys and their values were checked against an app Xcode built on the same Mac, for a device build and
            for a simulator build. No upload has been made with a bundle from this script.
          </p>
          <p>
            Beside the <code>Info.plist</code> and the binary the bundle holds <code>PrivacyInfo.xcprivacy</code>, the
            icon's files when it has one, and <code>embedded.mobileprovision</code> when it was signed with a profile.
            Keeping the app fast once it runs is on <a href={withBase("/fast-apps")}>Fast apps</a>.
          </p>
        </div>
      </section>

      <Pager prev={["/fast-apps", "Fast apps"]} next={["/architecture", "Architecture"]} />
    </>
  )
}
