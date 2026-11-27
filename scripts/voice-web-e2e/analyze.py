import sys
import wave

import numpy as np

RATE = 48000


def played(path):
    with open(path, 'rb') as f:
        raw = f.read()
    at = raw.find(b'data') + 8
    end = len(raw) - (len(raw) - at) % 4
    return np.frombuffer(raw[at:end], '<f4').astype(np.float64)


def talk(path):
    with wave.open(path) as w:
        x = np.frombuffer(w.readframes(w.getnframes()), np.int16) / 32768
        r = w.getframerate()
    return np.interp(np.arange(0, len(x) / r, 1 / RATE), np.arange(len(x)) / r, x)


def db(v):
    return 20 * np.log10(max(v, 1e-12))


speaker = played(sys.argv[1] + '/speaker.wav')
probe = talk(sys.argv[1] + '/talk.wav')
n = 1 << int(np.ceil(np.log2(len(speaker) + len(probe))))
gain = np.abs(np.fft.irfft(np.fft.rfft(speaker, n) * np.conj(np.fft.rfft(probe, n)), n)[:len(speaker)] / np.sum(probe * probe))
top = gain.max()
events = []
for k in np.argsort(gain)[::-1]:
    if gain[k] < top * 10 ** (-10 / 20):
        break
    if all(abs(k - e) > RATE * 2.5 for e in events):
        events.append(k)
events.sort()


def strongest(e, lo, hi):
    a, b = e + RATE * lo // 1000, e + RATE * hi // 1000
    j = a + int(np.argmax(gain[a:b]))
    return (j - e) * 1000 / RATE, db(gain[j] / gain[e])


extras = []
for e in events:
    if e + RATE * 2 > len(gain):
        continue
    (d1, g1), (d2, g2) = strongest(e, 550, 1050), strongest(e, 1250, 2000)
    extras.append(g1)
    print(f'repeat at {e / RATE:6.2f} s | strongest later echo +{d1:.0f} ms {g1:.1f} dB, then +{d2:.0f} ms {g2:.1f} dB')
if extras:
    print(f'median later echo {np.median(extras):.1f} dB re the repeat')
