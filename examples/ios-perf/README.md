# ios-perf

Measures a Rune app on an iPhone and prints one `PERF` line per number:

- `PERF first_frame_ms=… main_ms=… source=…`: from the kernel's process start time to the first frame, and to `main`. Rune and uikit modes print one line per source they have: `source=link_target`, the display-link target of the first frame Rune drew (`FrameEncoded::presents_at`), measured as native mode measures its own: a prediction of the refresh the frame aims at, not an observed presentation; and `source=presented`, the first frame Metal reports presented (`App::on_frame_presented`). iOS reports no presentation time until the app is active, after the launch animation, so on a phone `presented` reads about 300 ms after `link_target` on a warm launch; compare `link_target` with native mode's. The Simulator gives no presentation times, so there only `link_target` prints. `PERF marks …` lists Rune's launch marks on the same clock, among them `ios.scene.connected` (UIKit connected the window scene) `ios.view.window` (Rune's view joined its window), and `embed.renderer.begin` and `embed.renderer.ready` around the main thread's renderer build, which compiles the uber shader from WGSL on iOS. Together they split a cold launch into the system's part and Rune's.
- `PERF display max_fps=… disable_min_frame_duration=… reduce_motion=…`: what the screen offers, whether the bundle's `Info.plist` lets an iPhone run above 60 Hz, and whether Reduce Motion is on. With it on, `Tx::scroll_to_with` jumps rather than glides, so `--drive glide` presents only a few frames per fling.
- `PERF idle_footprint_mib=…`: `phys_footprint` from `task_info(TASK_VM_INFO)` after `--idle` seconds (5 by default) of doing nothing after the first frame Rune draws (in native mode, after the first display link refresh). In Rune and uikit modes it and `PERF after_scroll_footprint_mib=…` add `gpu_allocated_mib=…`, the system Metal device's `currentAllocatedSize`: the memory that device has allocated for buffers and textures (atlases, render targets). Core Animation's drawables may not be counted in it. Native mode leaves it out, since asking would make a Metal device it never has.
- With `--auto-scroll`, three fling-shaped scrolls of a 3,000-row list (about 8 s), then `PERF scroll mode=… source=… frames=… missed=… dropped=… p50_ms=… p99_ms=… max_ms=… period_ms=…`. `period_ms` is the refresh period at the screen's `max_fps` (8.33 ms on a 120 Hz iPhone, 16.67 ms on a 60 Hz one or the Simulator); `missed` counts frame intervals over 1.5 periods, `dropped` the refreshes they skipped.

Sources:

- `presented`: Metal's `presentedTime` for each frame Rune drew (Rune mode). The Simulator's drawables report none, so there it has no frames and Rune mode prints no `source=presented` first frame.
- `submit_target`: the display link refresh each Rune frame was submitted for; a gap here means Rune didn't submit a frame for that refresh, a gap only in `presented` means the frame was late on the GPU or the compositor.
- `link`: a `CADisplayLink`'s timestamps while the list moves, in both modes, so the two compare; `link_hz_p50` is the rate the link actually ran at.

`PERF rune_cpu` gives the update (events, view, layout) and encode milliseconds per frame.

Modes:

- `--mode rune` (default): a Rune list of `virtual_rows`, each a coloured circle and two lines of text. `--drive curve` (default) moves it each frame from `poll` along the same curve UIKit mode uses; `--drive glide` hands Rune three scroll glides to animate itself.
- `--mode uikit`: the same rows in a `UITableView` that reuses its cells, laid over the idle Rune view, moved along the same curve from a `CADisplayLink`.
- `--mode native`: the baseline Rune never starts in. The binary links Rune but runs its own `UIApplicationMain`, configures its window scene in code as a Rune app does, and puts the same `UITableView` in the window, moved the same way. Rune's runtime, renderer and Metal device never start, so `idle_footprint_mib` and the scroll numbers are UIKit's alone. Its `first_frame_ms` (`source=link_target`) is the target time of the first `CADisplayLink` refresh after the window is made visible: a prediction of the earliest refresh its first commit could reach, not an observed presentation. The window can show a refresh or more later, so it can read earlier than the real first frame. Rune mode's `source=link_target` is the same prediction for its first drawn frame, and its `source=presented` the time Metal reports a frame was first presented with a time.

`--exit` quits after the scroll summary; `--rows N` changes the list's length.

With `IOS_PERF_TRACE=1` in the environment, each lifecycle step also prints `PERF trace t_ms=… thread=… step=…`, on the same clock as `first_frame_ms`: `main` (with the arguments), `app_run`, `ready`, `visibility` and `focus` changes, `first_encoded`, `present_without_time` (a presented frame Metal gave no time for), `idle_wait`, `idle_sample`, `drive_asked` (from the idle thread), `drive_start`, `drive_running`, a `drive_beat` each second while scrolling (polls, frames encoded and presented, skipped presents, link ticks), `drive_end`, `settling`, `done`, `watch_link`, `watch_link_stop` and `finish`. Native mode adds `native_main`, `scene_connected`, `table_shown` and `first_link_tick`. A run that stops short shows the last step it reached.

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
xcrun devicectl device process launch --console --terminate-existing --device <device> \
  --environment-variables '{"IOS_PERF_TRACE": "1"}' com.example.iosperf --mode rune --auto-scroll --exit
```

Launch each mode several times and compare medians. `first_frame_ms` is a cold number only on the first launch after an install or reboot.
