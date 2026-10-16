"""Level sweep of a 1 kHz (coherent) sine through each clipper mode of the
reference: peak, true-peak, THD and inharmonic energy per drive level."""
import json, sys
import numpy as np
import goldclip as G
import metrics as M
import stimuli as S

SR = int(sys.argv[1]) if len(sys.argv) > 1 else 48000
N = 1 << 15
F0 = S.coherent_f(997.0, SR, N)
levels = np.arange(-30, 18.01, 1.0)
variants = {
    "Hard": dict(clipper_on_off=True, clipper_mode="Hard"),
    "Classic": dict(clipper_on_off=True, clipper_mode="Classic"),
    "Modern": dict(clipper_on_off=True, clipper_mode="Modern"),
    "Hard+guard": dict(clipper_on_off=True, clipper_mode="Hard", clip_guard=True),
    "Hard+filter": dict(clipper_on_off=True, clipper_mode="Hard", clipper_filter_on_off=True),
    "Classic+filter": dict(clipper_on_off=True, clipper_mode="Classic", clipper_filter_on_off=True),
}
out = {"sr": SR, "f0": F0, "levels": levels.tolist()}
for name, p in variants.items():
    rows = []
    for L in levels:
        x = S.sine(F0, SR, (N + SR // 4) / SR, 10 ** (L / 20))
        y = G.render(x, SR, **p)[SR // 4:]
        ta = M.tone_analysis(y, SR, F0)
        rows.append(dict(level=float(L), peak_db=float(M.db(M.peak(y))),
                         tp_db=float(M.db(M.true_peak(y))), rms_db=float(M.db(M.rms(y))), **ta))
    out[name] = rows
    print(name, " ".join(f"{r['level']:+.0f}:{r['peak_db']:.2f}/{r['thd_db']:.0f}/{r['inharm_db']:.0f}" for r in rows[::3]))
json.dump(out, open(f"../docs/data/gc_levels_{SR}.json", "w"), indent=0)
