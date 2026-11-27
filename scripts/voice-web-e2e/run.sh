#!/bin/bash
set -e
browser=$1 rate=$2 loop=$3 seconds=$4
export PULSE_SERVER=unix:/tmp/pulse.sock
pulseaudio -n --daemonize=yes --exit-idle-time=-1 2>/dev/null \
    -L "module-native-protocol-unix socket=/tmp/pulse.sock auth-anonymous=1" \
    -L "module-null-sink sink_name=speaker rate=$rate" \
    -L "module-null-sink sink_name=room rate=$rate" \
    -L "module-always-sink"
sleep 1
pactl set-default-sink speaker
pactl load-module module-remap-source master=room.monitor source_name=mic >/dev/null
pactl set-default-source mic
pactl load-module module-loopback source=speaker.monitor sink=room latency_msec=40 source_dont_move=true sink_dont_move=true >/dev/null
sleep 0.5
pactl set-sink-input-volume "$(pactl list short sink-inputs | awk '$4 == "module-loopback.c" { print $1 }')" 0.3428
espeak-ng -s 150 -w /tmp/speech.wav "one two three, testing the echo, four five six"
python3 /e2e/talk.py /tmp/speech.wav /out/talk.wav
python3 /e2e/serve.py /web > /out/log.txt 2>&1 &
profile=$(mktemp -d)
cp /e2e/user.js "$profile/"
timeout $((seconds + 1)) parecord --device=speaker.monitor --rate=48000 --channels=1 --format=float32le --file-format=wav /out/speaker.wav &
url="http://localhost:8000/harness.html?loop=$loop"
if [ "$browser" = chromium ]; then
    timeout "$seconds" /chromium/chrome --headless=new --no-sandbox --no-first-run --user-data-dir="$profile" \
        --use-fake-ui-for-media-stream --autoplay-policy=no-user-gesture-required "$url" >/out/browser.txt 2>&1 &
else
    timeout "$seconds" /opt/firefox/firefox --headless --no-remote --profile "$profile" "$url" >/out/browser.txt 2>&1 &
fi
if [ "$loop" = 1 ]; then
    (sleep 8; while true; do paplay --device=room /out/talk.wav; sleep 6; done) &
fi
sleep $((seconds + 2))
