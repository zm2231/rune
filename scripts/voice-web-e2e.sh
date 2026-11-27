#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
usage="usage: voice-web-e2e.sh [--browser firefox|chromium] [--rate HZ]

Runs voice-loopback's web build in headless Firefox or Chromium (CHROMIUM_DIR: a chrome-linux64
folder, such as Playwright's) in Docker, against a PulseAudio server at HZ (default 44100) whose
microphone hears the speaker at -9.3 dB. A voice-like probe talks into the microphone every 6 s,
and the speaker's recording is searched for echoes of it after the intended repeat. Fails when
the median later echo is above -15 dB."
browser=firefox
rate=44100
while (($#)); do
    case "$1" in
        --browser) browser=${2:?}; shift 2 ;;
        --rate) rate=${2:?}; shift 2 ;;
        -h|--help) echo "$usage"; exit 0 ;;
        *) echo "$usage" >&2; exit 2 ;;
    esac
done
here=$root/scripts/voice-web-e2e
web=$root/target/voice-web-e2e
docker build -q -t rune-voice-web-e2e "$here" >/dev/null
scripts/web-build.sh voice-loopback --no-html --out target/voice-web-e2e >/dev/null
cp "$here/harness.html" "$web/"
mounts=(-v "$here:/e2e:ro" -v "$web:/web:ro")
if [[ $browser == chromium ]]; then
    mounts+=(-v "${CHROMIUM_DIR:?set CHROMIUM_DIR to a chrome-linux64 folder}:/chromium:ro")
fi
out=$(mktemp -d)
trap 'docker run --rm -v "$out:/out" ubuntu:24.04 rm -rf /out/loop /out/burst; rm -rf "$out"' EXIT
mkdir -p "$out/loop" "$out/burst"
docker run --rm "${mounts[@]}" -v "$out/burst:/out" rune-voice-web-e2e /e2e/run.sh "$browser" "$rate" 0 20
docker run --rm "${mounts[@]}" -v "$out/loop:/out" rune-voice-web-e2e /e2e/run.sh "$browser" "$rate" 1 40
echo "$browser, device at $rate Hz"
echo "states: $(grep -oE 'state [A-Za-z]+' "$out/loop/log.txt" | head -3 | cut -d' ' -f2 | paste -sd' ')"
echo "burst only: $(grep -oE 'burst echo .*' "$out/burst/log.txt" | tail -1)"
report=$(docker run --rm -v "$here:/e2e:ro" -v "$out/loop:/out" rune-voice-web-e2e python3 /e2e/analyze.py /out)
echo "$report"
median=$(sed -n 's/^median later echo \(-\?[0-9.]*\) dB.*/\1/p' <<< "$report")
[[ -n $median ]] || { echo "no repeat of the probe was heard" >&2; exit 1; }
awk -v m="$median" 'BEGIN { exit !(m < -15) }' || { echo "echo of the playback gets through: median later echo $median dB" >&2; exit 1; }
echo "voice web e2e: ok"
