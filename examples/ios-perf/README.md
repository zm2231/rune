# ios-perf

Measures a Rune app on an iPhone and prints one `PERF` line per number:

- `PERF first_frame_ms=… main_ms=… source=…`: from the kernel's process start time to the first frame on screen, and to `main`. In Rune mode the first frame is the one Metal reports presented (`App::on_frame_presented`, `source=presented`); native mode's differs (below). `PERF marks …` lists Rune's launch marks on the same clock.
- `PERF display max_fps=… disable_min_frame_duration=…`: what the screen offers and whether the bundle's `Info.plist` lets an iPhone run above 60 Hz.
- `PERF idle_footprint_mib=…`: `phys_footprint` from `task_info(TASK_VM_INFO)` after `--idle` seconds (5 by default) of doing nothing after the first frame.
- With `--auto-scroll`, three fling-shaped scrolls of a 3,000-row list (about 8 s), then `PERF scroll mode=… source=… frames=… missed=… dropped=… p50_ms=… p99_ms=… max_ms=…`. `missed` counts frame intervals over 1.5 × 8.33 ms, `dropped` the refreshes they skipped.

Sources:

- `presented`: Metal's `presentedTime` for each frame Rune drew (Rune mode).
- `submit_target`: the display link refresh each Rune frame was submitted for; a gap here means Rune didn't submit a frame for that refresh, a gap only in `presented` means the frame was late on the GPU or the compositor.
- `link`: a `CADisplayLink`'s timestamps while the list moves, in both modes, so the two compare; `link_hz_p50` is the rate the link actually ran at.

`PERF rune_cpu` gives the update (events, view, layout) and encode milliseconds per frame.

Modes:

- `--mode rune` (default): a Rune list of `virtual_rows`, each a coloured circle and two lines of text. `--drive curve` (default) moves it each frame from `poll` along the same curve UIKit mode uses; `--drive glide` hands Rune three scroll glides to animate itself.
- `--mode uikit`: the same rows in a `UITableView` that reuses its cells, laid over the idle Rune view, moved along the same curve from a `CADisplayLink`.
- `--mode native`: the baseline Rune never starts in. The binary links Rune but runs its own `UIApplicationMain`, configures its window scene in code as a Rune app does, and puts the same `UITableView` in the window, moved the same way. Rune's runtime, renderer and Metal device never start, so `idle_footprint_mib` and the scroll numbers are UIKit's alone. Its `first_frame_ms` (`source=link_target`) is the target time of the first `CADisplayLink` refresh after the window is made visible: a prediction of the earliest refresh its first commit could reach, not an observed presentation. The window can show a refresh or more later, so it can read earlier than the real first frame. Rune's (`source=presented`) is the time Metal reports the first frame was actually presented.

`--exit` quits after the scroll summary; `--rows N` changes the list's length.

## Run it on a device

```sh
cargo build -p ios-perf --target aarch64-apple-ios --profile perf
scripts/ios-app --name IosPerf --id com.example.iosperf --bin target/aarch64-apple-ios/perf/ios-perf \
  --device <device> --profile <profile.mobileprovision> --no-launch
xcrun devicectl device process launch --console --terminate-existing --device <device> com.example.iosperf \
  --mode rune --auto-scroll --exit
xcrun devicectl device process launch --console --terminate-existing --device <device> com.example.iosperf \
  --mode uikit --auto-scroll --exit
xcrun devicectl device process launch --console --terminate-existing --device <device> com.example.iosperf \
  --mode native --auto-scroll --exit
```

Launch each mode several times and compare medians. `first_frame_ms` is a cold number only on the first launch after an install or reboot.
