#!/usr/bin/env python3
"""A short ascending coin chime, using only the Python standard library."""
import math
from pathlib import Path
import struct
import wave

RATE = 44100

def generate():
    samples = []
    for i in range(int(RATE * 0.55)):
        t = i / RATE
        v = 0.0
        for start, frequency in [(0.0, 1046.5), (0.09, 1568.0), (0.15, 2093.0)]:
            age = t - start
            if age < 0:
                continue
            envelope = min(age / 0.003, 1.0) * math.exp(-age / 0.09)
            v += envelope * (math.sin(2 * math.pi * frequency * age)
                + 0.25 * math.sin(2 * math.pi * frequency * 2.76 * age)
                + 0.10 * math.sin(2 * math.pi * frequency * 4.07 * age)) * 0.24
        samples.append(max(-32767, min(32767, round(v * 32767))))
    path = Path(__file__).resolve().parents[2] / "game/assets/audio/sfx/loot.wav"
    with wave.open(str(path), "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(RATE)
        wav.writeframes(struct.pack(f"<{len(samples)}h", *samples))
    print(path)

if __name__ == "__main__":
    generate()
