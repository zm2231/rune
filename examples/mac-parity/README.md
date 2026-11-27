# Mac parity

macOS platform pieces in one small app: a list of files you can preview with Quick Look.

```sh
cargo run -p mac-parity                # three sample text files in the temp folder
cargo run -p mac-parity -- a.pdf b.png # or your own files
```

Up and Down select a file. Space opens Quick Look on it; with the panel open, Up and Down
show the newly selected file, and space closes it (from the window or the panel). The line
under the title follows `Cx::quick_look_open`, so closing the panel with its close button or
Escape updates it too.

## Services

Services need an app bundle whose `Info.plist` declares them, so install the example first:

```sh
examples/mac-parity/install.sh
```

It builds a release binary, installs `~/Applications/Mac Parity.app` with `scripts/mac-app`,
merging `Info.extra.plist` (two `NSServices` entries, both with `NSMessage` `runeService`), and
flushes the Services cache. Then:

- Select text in TextEdit or Safari and choose *App menu ▸ Services ▸ Send Text to Mac Parity*.
  The window lists `text: text "…"` under "Received from Services".
- Select files in Finder and choose *Services ▸ Send Files to Mac Parity* (right-click ▸
  Quick Actions/Services). They are listed as `files: file …` and join the file list, ready for
  Quick Look.
- Quit the app and use a service again: it launches and the input still arrives.
