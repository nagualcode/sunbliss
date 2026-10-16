"""Test-signal generators. All return float64 mono arrays."""
from __future__ import annotations

import numpy as np

RATES = (44100, 48000, 96000)


def sine(f, sr, dur=1.0, amp=1.0, phase=0.0, fade=0.0):
    t = np.arange(int(dur * sr)) / sr
    x = amp * np.sin(2 * np.pi * f * t + phase)
    if fade > 0:
        x *= fade_env(len(x), int(fade * sr))
    return x


def coherent_f(f, sr, n):
    """Nearest frequency with an integer number of cycles in n samples."""
    return round(f * n / sr) * sr / n


def fade_env(n, nf):
    e = np.ones(n)
    r = 0.5 - 0.5 * np.cos(np.linspace(0, np.pi, nf))
    e[:nf] = r; e[-nf:] = r[::-1]
    return e


def two_tone(f1, f2, sr, dur=1.0, amp=1.0, ratio=1.0):
    t = np.arange(int(dur * sr)) / sr
    a2 = amp * ratio / (1 + ratio)
    a1 = amp / (1 + ratio)
    return a1 * np.sin(2 * np.pi * f1 * t) + a2 * np.sin(2 * np.pi * f2 * t)


def smpte(sr, dur=1.0, amp=1.0):
    """SMPTE IMD: 60 Hz + 7 kHz at 4:1."""
    t = np.arange(int(dur * sr)) / sr
    return amp * (0.8 * np.sin(2 * np.pi * 60 * t) + 0.2 * np.sin(2 * np.pi * 7000 * t))


def exp_sweep(sr, f1=20.0, f2=None, dur=4.0, amp=1.0):
    """Farina exponential sweep and its inverse filter."""
    f2 = f2 or sr / 2 * 0.95
    n = int(dur * sr)
    t = np.arange(n) / sr
    L = dur / np.log(f2 / f1)
    x = amp * np.sin(2 * np.pi * f1 * L * (np.exp(t / L) - 1))
    x *= fade_env(n, int(0.01 * sr))
    inv = x[::-1] * np.exp(-t / L)
    return x, inv, L


def burst(f, sr, on=0.05, off=0.25, n=6, amp=1.0, pre=0.1):
    seg_on = sine(f, sr, on, amp, fade=0.002)
    seg_off = np.zeros(int(off * sr))
    return np.concatenate([np.zeros(int(pre * sr))] + [np.concatenate([seg_on, seg_off])] * n)


def step(sr, dur=0.5, amp=0.5, at=0.1):
    x = np.zeros(int(dur * sr)); x[int(at * sr):] = amp
    return x


def noise(sr, dur=2.0, rms_db=-20.0, color="white", seed=1):
    rng = np.random.default_rng(seed)
    n = int(dur * sr)
    w = rng.standard_normal(n)
    if color == "pink":
        W = np.fft.rfft(w)
        f = np.fft.rfftfreq(n, 1 / sr); f[0] = f[1]
        w = np.fft.irfft(W / np.sqrt(f), n)
    w *= 10 ** (rms_db / 20) / np.sqrt(np.mean(w ** 2))
    return w
