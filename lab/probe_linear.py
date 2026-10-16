"""Low-level magnitude responses of the reference (noise at -40 dBFS)."""
import json, sys
import numpy as np
import matplotlib; matplotlib.use("Agg"); import matplotlib.pyplot as plt
import goldclip as G
import metrics as M
import stimuli as S

SR = int(sys.argv[1]) if len(sys.argv) > 1 else 48000
x = S.noise(SR, 8.0, -40.0)
variants = {
    "neutral": {},
    "Hard": dict(clipper_on_off=True, clipper_mode="Hard"),
    "Classic": dict(clipper_on_off=True, clipper_mode="Classic"),
    "Modern": dict(clipper_on_off=True, clipper_mode="Modern"),
    "Hard+filter": dict(clipper_on_off=True, clipper_mode="Hard", clipper_filter_on_off=True),
    "Classic+filter": dict(clipper_on_off=True, clipper_mode="Classic", clipper_filter_on_off=True),
    "Modern+filter": dict(clipper_on_off=True, clipper_mode="Modern", clipper_filter_on_off=True),
    "tone Classic": dict(box_tone_mode="Classic"),
    "tone Modern": dict(box_tone_mode="Modern"),
    "alchemy 2 (low lvl)": dict(alchemy_on_off=True, alchemy=2.0),
}
out = {"sr": SR}
fig, ax = plt.subplots(2, 1, figsize=(12, 9))
for name, p in variants.items():
    y = G.render(x, SR, **p)
    f, H = M.mag_response(x, y, SR, 1 << 15)
    mag = M.db(H)
    out[name] = dict(f=f[::4].tolist(), mag=mag[::4].tolist(), phase=np.degrees(np.angle(H[::4])).tolist())
    sel = [100, 1000, 3000, 5000, 8000, 10000, 15000, 18000, 20000, 21000, 22000]
    print(f"{name:22s}", " ".join(f"{s//1000 if s>=1000 else s}{'k' if s>=1000 else ''}:{mag[np.argmin(abs(f-s))]:+.2f}" for s in sel if s < SR/2))
    for a in ax:
        a.semilogx(f[1:], mag[1:], label=name)
ax[0].set_ylim(-8, 1); ax[1].set_ylim(-1.5, 0.3); ax[1].set_xlim(1000, SR/2)
for a in ax: a.grid(which="both"); a.legend(fontsize=7)
fig.suptitle(f"Reference low-level magnitude response, {SR} Hz")
fig.tight_layout(); fig.savefig(f"../docs/fig/gc_linear_{SR}.png", dpi=80)
json.dump(out, open(f"../docs/data/gc_linear_{SR}.json", "w"))
