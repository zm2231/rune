#!/usr/bin/env bash
set -euo pipefail
usage="usage: voice-pulse-e2e.sh [--loop-ms N]

Runs rune-media's real-sound-server voice tests in Docker against a PulseAudio server whose
microphone hears the speaker: directly, or with --loop-ms N through a loopback that delays it by
about N ms (a long echo path, as through a virtual audio cable). ALSA's default device is routed
to the same server, so the ALSA backend runs too. The tests cover echo cancellation, and speaker
underruns being counted while the echo stays cancelled."
loop_ms=0
while (($#)); do
    case "$1" in
        --loop-ms) loop_ms=${2:?}; shift 2 ;;
        -h|--help) echo "$usage"; exit 0 ;;
        *) echo "$usage" >&2; exit 2 ;;
    esac
done
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
bin=$(cargo test -p rune-media --lib --no-run --message-format=json 2>/dev/null \
  | jq -r 'select(.reason == "compiler-artifact" and .target.name == "rune_media" and .executable != null) | .executable' | tail -1)
[ -x "$bin" ] || { echo "couldn't build rune-media's tests" >&2; exit 1; }
docker run --rm -e LOOP_MS="$loop_ms" -v "$bin:/voice-tests:ro" ubuntu:24.04 bash -c '
  set -e
  apt-get update -qq >/dev/null
  DEBIAN_FRONTEND=noninteractive apt-get install -y -qq pulseaudio libasound2-plugins >/dev/null
  printf "pcm.!default { type pulse }\nctl.!default { type pulse }\n" > /etc/asound.conf
  pulseaudio -n --daemonize=yes --exit-idle-time=-1 \
    -L "module-null-sink sink_name=speaker" -L "module-null-sink sink_name=room" \
    -L "module-native-protocol-unix" -L "module-always-sink"
  sleep 1
  pactl set-default-sink speaker
  if [ "$LOOP_MS" -gt 0 ]; then
    pactl load-module module-loopback source=speaker.monitor sink=room latency_msec="$LOOP_MS" >/dev/null
    pactl set-default-source room.monitor
  else
    pactl set-default-source speaker.monitor
  fi
  /voice-tests voice::linux::tests:: --ignored --nocapture --test-threads=1
'
