#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
bin=$(cargo test -p rune-media --lib --no-run --message-format=json 2>/dev/null \
  | jq -r 'select(.reason == "compiler-artifact" and .target.name == "rune_media" and .executable != null) | .executable' | tail -1)
[ -x "$bin" ] || { echo "couldn't build rune-media's tests" >&2; exit 1; }
docker run --rm -v "$bin:/voice-tests:ro" ubuntu:24.04 bash -c '
  set -e
  apt-get update -qq >/dev/null && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq pulseaudio >/dev/null
  pulseaudio -n --daemonize=yes --exit-idle-time=-1 \
    -L "module-null-sink sink_name=speaker" -L "module-native-protocol-unix" -L "module-always-sink"
  sleep 1
  pactl set-default-sink speaker
  pactl set-default-source speaker.monitor
  /voice-tests voice::linux::tests::a_real_sound_server_loop_is_cancelled --ignored --nocapture
'
