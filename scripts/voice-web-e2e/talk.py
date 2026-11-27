import sys
import wave

import numpy as np

with wave.open(sys.argv[1]) as source:
    rate = source.getframerate()
    speech = np.frombuffer(source.readframes(source.getnframes()), np.int16) / 32768
envelope = np.sqrt(np.convolve(speech * speech, np.ones(rate // 50) / (rate // 50), 'same'))
noise = np.random.default_rng(7).standard_normal(len(speech))
spectrum = np.fft.rfft(noise)
freqs = np.fft.rfftfreq(len(noise), 1 / rate)
spectrum[(freqs < 200) | (freqs > 4000)] = 0
talk = np.fft.irfft(spectrum, len(noise)) * envelope
talk *= 0.5 / np.max(np.abs(talk))
with wave.open(sys.argv[2], 'wb') as out:
    out.setnchannels(1)
    out.setsampwidth(2)
    out.setframerate(rate)
    out.writeframes((talk * 32767).astype(np.int16).tobytes())
