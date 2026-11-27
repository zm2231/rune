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
