#!/usr/bin/env python3
"""Procedural sound effects (no samples): writes 16-bit WAVs to game/assets/audio/sfx.
Recipes: layered filtered noise + resonant bodies + synthetic reverb."""
import os
import numpy as np
from scipy import signal
from scipy.io import wavfile

SR = 44100
OUT = os.path.join(os.path.dirname(__file__), "..", "..", "game", "assets", "audio", "sfx")
rng = np.random.default_rng(1234)


def t(sec):
    return np.arange(int(sec * SR)) / SR


def white(sec):
    return rng.standard_normal(int(sec * SR))


def brown(sec):
    x = np.cumsum(white(sec))
    x = signal.lfilter([1], [1, -0.995], white(sec))
    return x / (np.max(np.abs(x)) + 1e-9)


def pink(sec):
    b = [0.049922035, -0.095993537, 0.050612699, -0.004408786]
    a = [1, -2.494956002, 2.017265875, -0.522189400]
    x = signal.lfilter(b, a, white(sec))
    return x / (np.max(np.abs(x)) + 1e-9)


def env_exp(sec, decay, attack=0.002):
    tt = t(sec)
    e = np.exp(-tt / decay)
    a = np.clip(tt / max(attack, 1e-5), 0, 1)
    return e * a


def bp(x, lo, hi, order=2):
    b, a = signal.butter(order, [lo / (SR / 2), min(hi / (SR / 2), 0.99)], btype="band")
    return signal.lfilter(b, a, x)


def lp(x, f, order=2):
    b, a = signal.butter(order, min(f / (SR / 2), 0.99), btype="low")
    return signal.lfilter(b, a, x)


def hp(x, f, order=2):
    b, a = signal.butter(order, f / (SR / 2), btype="high")
    return signal.lfilter(b, a, x)


def sweep_lp(x, f0, f1, curve=2.0):
    """Time-varying low-pass (block processed)."""
    n = len(x)
    out = np.zeros(n)
    blk = 512
    zi = None
    for i in range(0, n, blk):
        k = i / n
        f = f1 + (f0 - f1) * (1 - k) ** curve
        b, a = signal.butter(2, max(30, min(f, SR / 2 * 0.98)) / (SR / 2), btype="low")
        if zi is None:
            zi = signal.lfilter_zi(b, a) * 0
        seg, zi = signal.lfilter(b, a, x[i:i + blk], zi=zi)
        out[i:i + blk] = seg
    return out


def sine(f, sec, phase=0.0):
    return np.sin(2 * np.pi * f * t(sec) + phase)


def chirp(f0, f1, sec):
    tt = t(sec)
    f = f0 + (f1 - f0) * tt / sec
    return np.sin(2 * np.pi * np.cumsum(f) / SR)


def reverb(x, size=1.2, wet=0.25, damp=3000):
    n = int(size * SR)
    ir = white(size) * np.exp(-t(size) / (size / 6))
    ir = lp(ir, damp)
    # early reflections
    for d, g in ((0.011, 0.6), (0.023, 0.45), (0.037, 0.35), (0.053, 0.25)):
        k = int(d * SR)
        if k < n:
            ir[k] += g * 3
    ir /= np.sqrt(np.sum(ir ** 2)) + 1e-9
    w = signal.fftconvolve(x, ir)
    dry = np.concatenate([x, np.zeros(len(w) - len(x))])
    return dry * (1 - wet) + w * wet * 0.6


def pad(x, sec):
    n = int(sec * SR)
    return np.concatenate([x, np.zeros(max(0, n - len(x)))])[: max(n, len(x))]


def add_at(out, x, i):
    i = max(0, int(i))
    if i >= len(out):
        return
    m = min(len(x), len(out) - i)
    out[i:i + m] += x[:m]


def mix(*xs):
    n = max(len(x) for x in xs)
    out = np.zeros(n)
    for x in xs:
        out[: len(x)] += x
    return out


def fade_out(x, sec=0.05):
    n = min(len(x), int(sec * SR))
    x = x.copy()
    x[-n:] *= np.linspace(1, 0, n)
    return x


def trim_silence(x, thr=0.0015):
    idx = np.where(np.abs(x) > thr * np.max(np.abs(x)))[0]
    return x[: idx[-1] + 1] if len(idx) else x


def save(name, x, peak=0.89):
    x = np.asarray(x, dtype=np.float64)
    x = trim_silence(x)
    x = fade_out(x, 0.03)
    m = np.max(np.abs(x)) + 1e-9
    x = x / m * peak
    os.makedirs(OUT, exist_ok=True)
    wavfile.write(os.path.join(OUT, name + ".wav"), SR, (x * 32767).astype(np.int16))


# ----------------------------------------------------------------------------- weapons

def rifle(v):
    crack = bp(white(0.25), 900 + v * 200, 6000) * env_exp(0.25, 0.018 + v * 0.004, 0.0005)
    body = sine(130 + v * 15, 0.25) * env_exp(0.25, 0.03)
    snap = hp(white(0.02), 4000) * env_exp(0.02, 0.003, 0.0002)
    return reverb(mix(crack, body * 0.7, snap * 0.5), 1.0, 0.3)


def mg_burst(v, shots=5, rate=13.0):
    one = mix(bp(white(0.2), 700, 5000) * env_exp(0.2, 0.02, 0.0005), sine(110, 0.2) * env_exp(0.2, 0.025) * 0.8)
    out = np.zeros(int(SR * (shots / rate + 0.3)))
    for k in range(shots):
        i = int(k / rate * SR + rng.uniform(-0.004, 0.004) * SR)
        amp = 1.0 - 0.08 * k + rng.uniform(-0.05, 0.05)
        add_at(out, one * (amp), i)
    return reverb(out, 1.1, 0.28)


def sniper(v):
    crack = bp(white(0.4), 600, 7000) * env_exp(0.4, 0.035, 0.0005)
    boom = lp(brown(0.6), 400) * env_exp(0.6, 0.12) * 0.8
    return reverb(mix(crack, boom), 1.8, 0.35)


def cannon(v):
    blast = sweep_lp(white(1.2), 5000, 250) * env_exp(1.2, 0.18, 0.001)
    thump = sine(55 + v * 6, 1.2) * env_exp(1.2, 0.22) * 1.2
    crack = bp(white(0.05), 1500, 8000) * env_exp(0.05, 0.01, 0.0003)
    return reverb(mix(blast, thump, crack * 0.8), 2.0, 0.35, 2000)


def artillery(v):
    blast = sweep_lp(white(2.5), 3500, 120, 1.5) * env_exp(2.5, 0.45, 0.002)
    sub = sine(38 + v * 4, 2.5) * env_exp(2.5, 0.5) * 1.4
    crack = bp(white(0.08), 1200, 7000) * env_exp(0.08, 0.015, 0.0003)
    return reverb(mix(blast, sub, crack), 3.0, 0.4, 1500)


def mortar(v):
    foomp = lp(white(0.6), 600) * env_exp(0.6, 0.08, 0.004)
    tube = sine(95 + v * 10, 0.6) * env_exp(0.6, 0.09)
    return reverb(mix(foomp, tube * 0.8), 1.4, 0.3)


def missile(v):
    sec = 1.3
    n = white(sec)
    whoosh = sweep_lp(n, 800, 5000, 0.7) * np.clip(t(sec) / 0.05, 0, 1) * np.exp(-t(sec) / 0.6)
    ignite = bp(white(0.15), 300, 3000) * env_exp(0.15, 0.04)
    return reverb(mix(whoosh, ignite), 1.2, 0.25)


def flak(v):
    one = mix(lp(white(0.3), 2500) * env_exp(0.3, 0.05, 0.001), sine(80, 0.3) * env_exp(0.3, 0.06))
    out = np.zeros(int(SR * 1.2))
    for k in range(4):
        i = int(k * 0.11 * SR)
        add_at(out, one * ((1 - 0.1 * k)), i)
    return reverb(out, 1.4, 0.3)


def torpedo(v):
    sec = 1.5
    return mix(lp(white(sec), 900) * np.exp(-t(sec) / 0.7) * np.clip(t(sec) / 0.1, 0, 1), bp(white(sec), 2000, 6000) * np.exp(-t(sec) / 0.3) * 0.2)

# ----------------------------------------------------------------------------- impacts

def explosion(v, size):
    sec = 1.0 + size * 1.2
    body = sweep_lp(white(sec), 6000 - size * 1500, 90 + 40 * v, 1.3) * env_exp(sec, 0.25 + size * 0.25, 0.002)
    sub = sine(42 + v * 5 - size * 6, sec) * env_exp(sec, 0.3 + size * 0.2) * (0.8 + size * 0.5)
    crackle = np.zeros(int(sec * SR))
    for k in range(int(25 + size * 30)):
        i = int(rng.uniform(0.05, sec * 0.7) * SR)
        c = hp(white(0.01), 2500) * env_exp(0.01, 0.002) * rng.uniform(0.05, 0.3) * np.exp(-i / SR / (sec * 0.4))
        add_at(crackle, c, i)
    return reverb(mix(body, sub, crackle), 1.5 + size, 0.35, 1800)


def nuclear_blast():
    """Pressure crack, broad low-frequency blast and a long rolling thunder tail."""
    sec = 14.0
    tt = t(sec)
    pressure = sweep_lp(white(sec), 5200, 100, 0.7) * env_exp(sec, 1.25, 0.001)
    low = lp(pink(sec), 220) * (1 - np.exp(-tt / 0.06)) * np.exp(-tt / 4.2)
    thump = chirp(65, 28, sec) * env_exp(sec, 0.65, 0.004)
    rumble = lp(brown(sec), 150) * (1 - np.exp(-tt / 0.35)) * np.exp(-tt / 5.0)
    rolling = np.zeros(len(tt))
    for delay, strength in [(0.32, 0.6), (0.85, 0.45), (1.7, 0.32), (3.0, 0.23), (4.6, 0.12)]:
        wave = lp(pink(4.0), 280) * env_exp(4.0, 1.1, 0.05)
        add_at(rolling, wave * strength, int(delay * SR))
    return fade_out(mix(pressure * 1.6, low * 2.5, thump * 0.55, rumble * 2.0, rolling), 1.8)


def aircraft_breakup():
    sec = 2.8
    tear = sweep_lp(white(sec), 7000, 500) * env_exp(sec, 0.4, 0.002)
    metal = sum(chirp(f, f * 0.65, sec) / (i + 2) for i, f in enumerate([180, 287, 463, 719]))
    return reverb(tear + metal * env_exp(sec, 0.6) * 0.4, 1.4, 0.2)


def ship_sinking():
    sec = 10.0
    tt = t(sec)
    groan = sum(chirp(f, f * 0.6, sec) / (i + 1) for i, f in enumerate([48, 77, 133, 219]))
    groan *= (0.3 + 0.7 * np.sin(tt * 1.7) ** 2) * np.exp(-tt / 4.5) * np.clip(tt / 0.3, 0, 1)
    water = bp(pink(sec), 180, 1800) * np.clip(tt / 1.2, 0, 1) * np.exp(-tt / 5.0)
    return fade_out(groan * 0.35 + water, 1.0)


def bullet_impact(v):
    return mix(bp(white(0.12), 500, 4000) * env_exp(0.12, 0.02), lp(white(0.12), 300) * env_exp(0.12, 0.03) * 0.5)


def splash(v):
    sec = 1.6
    s = hp(white(sec), 700) * env_exp(sec, 0.25, 0.005)
    bubbles = np.zeros(int(sec * SR))
    for k in range(40):
        i = int(rng.uniform(0.1, 1.2) * SR)
        f = rng.uniform(400, 1400)
        b = chirp(f, f * 1.6, 0.04) * env_exp(0.04, 0.01) * 0.15
        add_at(bubbles, b, i)
    thud = lp(white(0.4), 250) * env_exp(0.4, 0.06)
    return reverb(mix(s * 0.8, bubbles, thud), 1.0, 0.2)


def collapse(v):
    sec = 4.0
    rumble = lp(brown(sec), 300) * np.exp(-t(sec) / 1.6) * np.clip(t(sec) / 0.2, 0, 1)
    debris = np.zeros(int(sec * SR))
    for k in range(120):
        i = int(rng.uniform(0.0, 3.0) * SR)
        d = bp(white(0.06), rng.uniform(300, 1500), 5000) * env_exp(0.06, 0.012) * rng.uniform(0.1, 0.5)
        add_at(debris, d, i)
    return reverb(mix(rumble * 1.2, debris, explosion(v, 1.0) * 0.6), 2.0, 0.35)

# ----------------------------------------------------------------------------- vehicles & ambience

def jet_flyby(v):
    sec = 3.2
    n = pink(sec)
    tt = t(sec)
    mid = sec * 0.45
    amp = 1 / (1 + ((tt - mid) / 0.35) ** 2)
    roar = sweep_lp(n, 7000, 1200, 1.0) * amp
    whine = chirp(2600, 1700, sec) * amp * 0.06
    return mix(roar, whine)


def heli_loop(v):
    sec = 2.0
    n = lp(white(sec), 1200)
    tt = t(sec)
    chop = 0.45 + 0.55 * (np.sin(2 * np.pi * 17.5 * tt) > 0.6)
    turb = bp(white(sec), 2000, 5000) * 0.12
    x = n * chop + turb
    # make it loop seamlessly
    k = int(0.05 * SR)
    x[:k] = x[:k] * np.linspace(0, 1, k) + x[-k:] * np.linspace(1, 0, k)
    return x[:-k]


def ocean_loop(v):
    sec = 12.0
    tt = t(sec)
    x = lp(pink(sec), 900) * (0.6 + 0.4 * np.sin(2 * np.pi * tt / 6.0) ** 2)
    hiss = hp(white(sec), 3000) * 0.05 * (0.5 + 0.5 * np.sin(2 * np.pi * tt / 6.0 + 1) ** 4)
    x = x + hiss
    k = int(1.0 * SR)
    x[:k] = x[:k] * np.linspace(0, 1, k) + x[-k:] * np.linspace(1, 0, k)
    return x[:-k]


def birds(v):
    sec = 6.0
    out = np.zeros(int(sec * SR))
    for k in range(10):
        i = int(rng.uniform(0, 5.5) * SR)
        f = rng.uniform(2500, 4500)
        n = rng.integers(2, 6)
        for j in range(n):
            c = chirp(f, f * rng.uniform(1.1, 1.5), 0.06) * env_exp(0.06, 0.02, 0.005) * 0.3
            ii = i + int(j * 0.09 * SR)
            add_at(out, c, ii)
    return reverb(out, 1.5, 0.3)

# ----------------------------------------------------------------------------- work & UI

def hammer(v):
    hit = mix(bp(white(0.15), 800, 3500) * env_exp(0.15, 0.015, 0.0005), sine(420 + v * 40, 0.15) * env_exp(0.15, 0.04) * 0.6)
    return reverb(hit, 0.6, 0.2)


def chop(v):
    hit = mix(bp(white(0.2), 300, 2500) * env_exp(0.2, 0.02, 0.0005), sine(180 + v * 20, 0.2) * env_exp(0.2, 0.05) * 0.8)
    return reverb(hit, 0.7, 0.2)


def pick(v):
    hit = mix(hp(white(0.15), 2000) * env_exp(0.15, 0.01, 0.0003), sine(1800 + v * 200, 0.15) * env_exp(0.15, 0.06) * 0.4)
    return reverb(hit, 0.7, 0.25)


def bell(freqs, sec=1.2, decay=0.4):
    x = np.zeros(int(sec * SR))
    for k, f in enumerate(freqs):
        part = sum(sine(f * h, sec) * (0.6 / (h * h)) for h in (1, 2.01, 3.02))
        start = int(k * 0.11 * SR)
        e = env_exp(sec, decay, 0.003)
        x[start:] += (part * e)[: len(x) - start]
    return reverb(x, 1.2, 0.3)


def ui_click(v):
    return mix(sine(1200, 0.05) * env_exp(0.05, 0.008, 0.0005), hp(white(0.02), 3000) * env_exp(0.02, 0.003) * 0.3)


def alert_variant(v):
    """Four distinct attack alerts: brass horn, klaxon, radio pips, war drum."""
    if v == 0:
        sec = 1.4
        f = 196.0
        x = sum(signal.sawtooth(2 * np.pi * f * h * t(sec)) / h for h in (1, 1.5, 2))
        x = lp(x, 1600) * np.clip(t(sec) / 0.08, 0, 1) * np.clip((sec - t(sec)) / 0.4, 0, 1)
        return reverb(x, 1.6, 0.35)
    if v == 1:
        sec = 1.2
        tt = t(sec)
        x = signal.square(2 * np.pi * 330 * tt) * (np.sin(2 * np.pi * 3 * tt) > 0)
        x = bp(x, 300, 2500) * np.clip((sec - tt) / 0.2, 0, 1)
        return reverb(x * 0.6, 0.9, 0.25)
    if v == 2:
        out = np.zeros(int(1.0 * SR))
        for k in range(3):
            p = sine(1400, 0.09) * env_exp(0.09, 0.06, 0.004)
            add_at(out, p, int(k * 0.16 * SR))
        add_at(out, bp(white(0.15), 1500, 4000) * env_exp(0.15, 0.05) * 0.3, 0)
        return reverb(out, 0.7, 0.2)
    sec = 1.6
    out = np.zeros(int(sec * SR))
    for k, tm in enumerate((0.0, 0.25, 0.5, 0.62)):
        hit = mix(sine(70, 0.5) * env_exp(0.5, 0.18), lp(white(0.5), 400) * env_exp(0.5, 0.05) * 0.6)
        add_at(out, hit * (1.0 if k != 2 else 0.7), int(tm * SR))
    return reverb(out, 1.4, 0.3)


def alert(v):
    sec = 1.6
    tt = t(sec)
    f = 620 + 140 * (np.sin(2 * np.pi * 2.5 * tt) > 0)
    x = np.sin(2 * np.pi * np.cumsum(f) / SR)
    x = signal.sawtooth(2 * np.pi * np.cumsum(f) / SR) * 0.4 + x * 0.6
    x = lp(x, 2500) * np.clip(tt / 0.05, 0, 1) * np.clip((sec - tt) / 0.2, 0, 1)
    return reverb(x, 0.8, 0.2)


def sting(major=True):
    root = 261.6
    chord = [root, root * (1.26 if major else 1.19), root * 1.5, root * 2]
    sec = 3.5
    x = np.zeros(int(sec * SR))
    for k, f in enumerate(chord):
        tone = sum(signal.sawtooth(2 * np.pi * f * h * t(sec)) * (0.3 / h) for h in (1, 2))
        tone = lp(tone, 1800) * np.clip(t(sec) / 0.3, 0, 1) * np.exp(-t(sec) / 1.6)
        x += tone
    timp = sine(65, sec) * env_exp(sec, 0.6) * 1.5
    return reverb(mix(x, timp), 2.5, 0.4)


def main():
    for v in range(4):
        save(f"rifle_{v}", rifle(v))
        save(f"bullet_hit_{v}", bullet_impact(v), 0.5)
    for v in range(3):
        save(f"mg_{v}", mg_burst(v))
        save(f"cannon_{v}", cannon(v))
        save(f"missile_{v}", missile(v))
        save(f"explosion_small_{v}", explosion(v, 0.0))
        save(f"explosion_med_{v}", explosion(v, 0.6))
        save(f"splash_{v}", splash(v))
        save(f"hammer_{v}", hammer(v), 0.6)
        save(f"chop_{v}", chop(v), 0.6)
        save(f"pick_{v}", pick(v), 0.5)
    for v in range(2):
        save(f"sniper_{v}", sniper(v))
        save(f"artillery_{v}", artillery(v))
        save(f"mortar_{v}", mortar(v))
        save(f"flak_{v}", flak(v))
        save(f"explosion_big_{v}", explosion(v, 1.2))
        save(f"jet_{v}", jet_flyby(v))
    save("nuclear_blast", nuclear_blast())
    save("aircraft_breakup", aircraft_breakup())
    save("ship_sinking", ship_sinking())
    save("torpedo", torpedo(0))
    save("collapse", collapse(0))
    save("heli_loop", heli_loop(0), 0.6)
    save("ambient_ocean", ocean_loop(0), 0.5)
    save("ambient_birds", birds(0), 0.4)
    save("ui_click", ui_click(0), 0.5)
    save("notify", bell([880, 1318.5]), 0.5)
    save("complete", bell([659.3, 880, 1318.5], 1.6, 0.5), 0.5)
    save("research", bell([523.3, 784, 1046.5, 1568], 2.0, 0.6), 0.5)
    save("alert", alert(0), 0.6)
    for v in range(4):
        save(f"alert_{v}", alert_variant(v), 0.6)
    save("victory", sting(True), 0.8)
    save("defeat", sting(False), 0.8)
    print("wrote", len(os.listdir(OUT)), "sounds to", os.path.abspath(OUT))


if __name__ == "__main__":
    main()
