"""Alchemy (dynamic HF peak softener) black-box probes."""
import json
import numpy as np
import goldclip as G
import metrics as M
import stimuli as S

SR = 48000
N = 1 << 14
base = dict(clipper_on_off=True, clipper_mode="Hard")
alc = dict(base, alchemy_on_off=True, alchemy=2.0)
out = {}


def tone_gain(f, L, p, extra=None):
    fc = S.coherent_f(f, SR, N)
    x = S.sine(fc, SR, (N + SR // 4) / SR, 10 ** (L / 20))
    if extra is not None:
        x = x + extra[: len(x)]
    y = G.render(x, SR, **p)[SR // 4:]
    xx = x[SR // 4:]
    w = np.hanning(N)
    k = int(round(fc * N / SR))
    X = np.abs(np.fft.rfft(xx * w))[k - 2:k + 3].sum()
    Y = np.abs(np.fft.rfft(y * w))[k - 2:k + 3].sum()
    return 20 * np.log10(Y / X)

# 1) static: 8 kHz level sweep
lv = np.arange(-20, 0.01, 1.0)
out["static_8k"] = [[float(L), tone_gain(8000, L, alc) - tone_gain(8000, L, base)] for L in lv]
print("8k static:", " ".join(f"{a:+.0f}:{b:+.2f}" for a, b in out["static_8k"]))
# 2) frequency curve at -0.5 dBFS
fr = [200, 500, 1000, 1500, 2000, 3000, 4000, 5000, 6000, 8000, 10000, 12000, 15000, 18000]
out["freq_m0.5"] = [[f, tone_gain(f, -0.5, alc) - tone_gain(f, -0.5, base)] for f in fr]
print("freq @-0.5:", " ".join(f"{a}:{b:+.2f}" for a, b in out["freq_m0.5"]))
# 3) detector: 8k at -30 dBFS riding on a loud 60 Hz tone at various peaks
for Lb in [-12, -6, -3, -1, 0]:
    bass = S.sine(60.0, SR, 2.0, 10 ** (Lb / 20))
    g = tone_gain(8000, -30, alc, bass) - tone_gain(8000, -30, base, bass)
    out.setdefault("detector_bass", []).append([Lb, g])
print("8k@-30 with 60Hz bass:", out["detector_bass"])
# 4) amount scaling at -0.5 dBFS 8k
out["amount"] = [[a, tone_gain(8000, -0.5, dict(base, alchemy_on_off=True, alchemy=a)) - tone_gain(8000, -0.5, base)] for a in [0.5, 1.0, 1.5, 2.0]]
print("amount:", out["amount"])
# 5) ceiling-relative? ceiling -6, 8k at -6.5
out["ceiling_m6"] = tone_gain(8000, -6.5, dict(alc, ceiling=-6.0)) - tone_gain(8000, -6.5, dict(base, ceiling=-6.0))
print("ceiling -6, 8k @ -6.5:", out["ceiling_m6"])
json.dump(out, open("../docs/data/gc_alchemy.json", "w"), indent=0)
