#!/usr/bin/env bash
# Builds a Rune example for the iOS Simulator, bundles it as an .app with its Info.plist, and with
# --device installs and launches it on that booted simulator. Needs macOS with Xcode and
# `rustup target add aarch64-apple-ios-sim`.
#
#   scripts/ios-app.sh ios-hello                      # build and bundle
#   scripts/ios-app.sh ios-hello --device <udid|name> # and install and launch
#   scripts/ios-app.sh ios-hello --release
set -euo pipefail

usage() { echo "usage: $0 <example package> [--device <udid|name>] [--release]" >&2; exit 2; }

[[ $# -ge 1 ]] || usage
example=$1; shift
device=""
profile=dev
while [[ $# -gt 0 ]]; do
  case $1 in
    --device) [[ $# -ge 2 ]] || usage; device=$2; shift 2 ;;
    --release) profile=release; shift ;;
    *) usage ;;
  esac
done

root=$(cd "$(dirname "$0")/.." && pwd)
triple=aarch64-apple-ios-sim
target_dir=${CARGO_TARGET_DIR:-$root/target}
out_dir=$([[ $profile == dev ]] && echo debug || echo release)
min_ios=17.5
bundle_id="dev.rune.${example}"

flags=(-p "$example" --target "$triple")
[[ $profile == release ]] && flags+=(--release)
(cd "$root" && IPHONEOS_DEPLOYMENT_TARGET=$min_ios cargo build "${flags[@]}")

exe="$target_dir/$triple/$out_dir/$example"
app="$target_dir/$triple/$out_dir/$example.app"
rm -rf "$app"
mkdir -p "$app"
cp "$exe" "$app/$example"

# UILaunchScreen makes the app fill the screen rather than run letterboxed;
# UIApplicationSceneManifest opts into the scene lifecycle; and
# CADisableMinimumFrameDurationOnPhone lets a ProMotion iPhone's display link reach 120 Hz.
cat > "$app/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleExecutable</key><string>$example</string>
  <key>CFBundleIdentifier</key><string>$bundle_id</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>$example</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.0.1</string>
  <key>CFBundleSupportedPlatforms</key><array><string>iPhoneSimulator</string></array>
  <key>CFBundleVersion</key><string>1</string>
  <key>CADisableMinimumFrameDurationOnPhone</key><true/>
  <key>LSRequiresIPhoneOS</key><true/>
  <key>MinimumOSVersion</key><string>$min_ios</string>
  <key>UIApplicationSceneManifest</key>
  <dict>
    <key>UIApplicationSupportsMultipleScenes</key><false/>
  </dict>
  <key>UIDeviceFamily</key><array><integer>1</integer><integer>2</integer></array>
  <key>UILaunchScreen</key><dict/>
  <key>UISupportedInterfaceOrientations</key>
  <array>
    <string>UIInterfaceOrientationPortrait</string>
    <string>UIInterfaceOrientationLandscapeLeft</string>
    <string>UIInterfaceOrientationLandscapeRight</string>
  </array>
</dict>
</plist>
PLIST
plutil -lint "$app/Info.plist" >/dev/null
codesign --force --sign - "$app"
echo "$app"

if [[ -n $device ]]; then
  xcrun simctl install "$device" "$app"
  xcrun simctl launch --terminate-running-process "$device" "$bundle_id"
fi
