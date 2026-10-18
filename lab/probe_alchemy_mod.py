"""Alchemy dynamics: how the high-band gain follows a carrier.

Clipper off (no clip distortion), a carrier at level A (1.5 Hz .. 1 kHz)
plus a -40 dB probe near 15 kHz, off the carrier's harmonic grid. The
probe's gain with Alchemy at 2 dB is the mean band gain; its sidebands at
+-2f and +-4f measure how much of the carrier's waveform the gain follows.
Writes docs/data/alchemy_mod.json ({"ref": ..., "ours": ...}).
"""
import json

import numpy as np

import render as R

SR = 48000
N = 1 << 15
PROBE_BIN = 10240 + 7          # ~15.0 kHz
CARRIER_BINS = [1, 3, 11, 43, 171, 341, 683]   # ~1.5 Hz .. 1 kHz
LEVELS = [-1.0, -3.0, -6.0]


def sine(b, a, n):
    return a * np.sin(2 * np.pi * b * np.arange(n) / N)


def probe(fn):
    out = {}
    for A in LEVELS:
        for b in CARRIER_BINS:
            n = N + SR // 4
            x = sine(b, 10 ** (A / 20), n) + sine(PROBE_BIN, 1e-2, n)
            ys = [fn(x, SR, clip=False, contour=c)[-N:] for c in (0.0, 2.0)]
            Y = [np.abs(np.fft.rfft(y * np.hanning(N))) for y in ys]

            def amp(S, k):
                return S[k - 2:k + 3].max()
            g = 20 * np.log10(amp(Y[1], PROBE_BIN) / amp(Y[0], PROBE_BIN))
            sb = [float(20 * np.log10(amp(Y[1], PROBE_BIN + h * 2 * b) / amp(Y[1], PROBE_BIN))) if b > 2 else None
                  for h in (1, 2)]
            out[f"{A}_{b * SR / N:.1f}"] = dict(gain_db=float(g), sb2_db=sb[0], sb4_db=sb[1])
    return out


if __name__ == "__main__":
    res = {"ref": probe(R.ref), "ours": probe(R.ours)}
    for k in res["ref"]:
        a, b = res["ref"][k], res["ours"][k]
        f = lambda v: "  -  " if v is None else f"{v:6.1f}"
        print(f"{k:>12}: gain {a['gain_db']:6.2f} / {b['gain_db']:6.2f}   2f {f(a['sb2_db'])} / {f(b['sb2_db'])}")
    json.dump(res, open("../docs/data/alchemy_mod.json", "w"), indent=1)
