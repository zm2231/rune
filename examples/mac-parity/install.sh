#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
cargo build --release -p mac-parity --manifest-path "$root/Cargo.toml"
"$root/scripts/mac-app" --name "Mac Parity" --id dev.rune.mac-parity --exe mac-parity \
  --bin "$root/target/release/mac-parity" --plist "$root/examples/mac-parity/Info.extra.plist" "$@"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$HOME/Applications/Mac Parity.app"
/System/Library/CoreServices/pbs -update
