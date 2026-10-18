"""Alchemy (internally "MX" in the reference): instantaneous gain law.

The reference applies the high-band gain sample by sample from the
wide-band level. Probe: a soft-edged DC plateau at level L (re ceiling,
Hard clipper on so the level is what the stage sees) plus a small tone;
the tone's gain with Alchemy on / off is the band gain at that level.
At 8 kHz the high band is the whole tone, so that gain is k(L) itself; the
other frequencies give the split shape at the same k.
Writes docs/data/gc_alchemy_inst.json.
"""
import json

import numpy as np

import render as R
import stimuli as S

SR = 48000
N = 1 << 13
SEG = N + SR // 10


def probe_gain(fn, L, f, amount):
    fc = S.coherent_f(f, SR, N)
    rise = SR // 200
    env = np.ones(SEG)
    w = 0.5 - 0.5 * np.cos(np.pi * np.arange(rise) / rise)
    env[:rise] = w
    x = 10 ** (L / 20) * env + 1e-3 * S.sine(fc, SR, SEG / SR, 1.0)
    x = np.r_[x, np.zeros(SR // 20)]
    k = int(round(fc * N / SR))
    win = np.hanning(N)

    def tone(y):
        seg = y[SR // 10:SR // 10 + N]
        return np.abs(np.fft.rfft((seg - seg.mean()) * win))[k - 2:k + 3].sum()

    on = fn(x, SR, clip=True, mode="Hard", contour=amount)
    off = fn(x, SR, clip=True, mode="Hard", contour=0.0)
    return 20 * np.log10(tone(on) / tone(off))


LEVELS = [-20.0, -16.0] + [float(v) for v in np.arange(-14.0, 0.01, 0.5)]
AMOUNTS = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0]
FREQS = [500, 1000, 2000, 3000, 4000, 6000, 8000, 12000, 16000]

if __name__ == "__main__":
    import sys
    fn = R.ours if "ours" in sys.argv else R.ref
    out = dict(levels=LEVELS, amounts=AMOUNTS, law={}, freqs=FREQS, shape={})
    for a in AMOUNTS:
        out["law"][str(a)] = [round(float(probe_gain(fn, L, 8000, a)), 4) for L in LEVELS]
        print("amount", a, out["law"][str(a)])
    for L in (-6.0, -2.0, -0.5):
        out["shape"][str(L)] = [round(float(probe_gain(fn, L, f, 2.0)), 4) for f in FREQS]
        print("shape @", L, out["shape"][str(L)])
    name = "ours_alchemy_inst" if fn is R.ours else "gc_alchemy_inst"
    json.dump(out, open(f"../docs/data/{name}.json", "w"), indent=1)
