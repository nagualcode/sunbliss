"""Parity measurements (brief §7.1): same probes on reference and Sunbliss.

Each test returns a dict of numbers; `main` runs them, prints a table and
writes docs/data/parity.json (consumed by report.py)."""
from __future__ import annotations

import json
import sys

import numpy as np

import metrics as M
import render as R
import stimuli as S
from music_synth import mix

SR = 48000


def dc_steps(levels, seg=SR // 50, rise=SR // 500):
    """DC plateaus with 2 ms raised-cosine edges: a hard edge rings through
    any band-limited path, and that overshoot is not the static curve."""
    w = 0.5 - 0.5 * np.cos(np.pi * np.arange(rise) / rise)
    env = np.concatenate([w, np.ones(seg - 2 * rise), w[::-1], np.zeros(seg)])
    return np.concatenate([a * env for a in levels])


def dc_curve(fn, mode, levels=np.linspace(-1.3, 1.3, 53), **s):
    seg = SR // 50
    x = dc_steps(levels, seg)
    y = fn(x, SR, clip=True, mode=mode, **s)
    return np.array([np.median(y[i * 2 * seg + seg // 4:i * 2 * seg + seg // 2]) for i in range(len(levels))])


def t_static_clip():
    out = {}
    lv = np.linspace(-1.3, 1.3, 53)
    for mode in ["A", "B", "Hard"]:
        a, b = dc_curve(R.ref, mode), dc_curve(R.ours, mode)
        m = (np.abs(lv) >= 0.5)  # knee/clip region (-6 dB .. +2.3 dB re ceiling)
        err = np.abs(M.db(b[m]) - M.db(a[m]))
        out[mode] = dict(max_db=float(err.max()), mean_db=float(err.mean()))
    return out


def t_levels():
    """1 kHz sine, -12..+12 dB re ceiling: output RMS and THD per level."""
    out = {}
    N = 1 << 14
    f0 = S.coherent_f(997, SR, N)
    for mode in ["A", "B", "Hard"]:
        rows = []
        for L in np.arange(-12, 12.1, 2.0):
            x = S.sine(f0, SR, (N + SR // 4) / SR, 10 ** (L / 20))
            a, b = R.both(x, SR, clip=True, mode=mode)
            a, b = a[SR // 4:], b[SR // 4:]
            ta, tb = M.tone_analysis(a, SR, f0), M.tone_analysis(b, SR, f0)
            rows.append(dict(level=float(L), rms_ref=M.db(M.rms(a)), rms_ours=M.db(M.rms(b)),
                             thd_ref=ta["thd_db"], thd_ours=tb["thd_db"], inh_ref=ta["inharm_db"], inh_ours=tb["inharm_db"]))
        out[mode] = dict(rows=rows, max_rms_diff_db=float(max(abs(r["rms_ref"] - r["rms_ours"]) for r in rows)))
    return out


def t_saturation():
    """10 Hz sine at 0.999 FS, clip off: static curve per curve/amount."""
    out = {}
    n = SR // 2 + SR // 8
    t = np.arange(n) / SR
    x = 0.999 * np.sin(2 * np.pi * 10 * t)
    s0, per = SR // 8, SR // 10
    for curve in ["A", "B"]:
        errs = []
        for g in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]:
            a, b = R.both(x, SR, sat=g, curve=curve)
            xs, a, b = x[s0:s0 + per], a[s0:s0 + per], b[s0:s0 + per]
            m = np.abs(xs) > 10 ** (-30 / 20)
            # compare as functions of the input (both are memoryless)
            errs.append(float(np.max(np.abs(M.db(b[m]) - M.db(a[m])))))
        out[curve] = dict(max_db_per_amount=errs, max_db=max(errs))
    return out


def t_unity():
    x = S.sine(997, SR, 0.5, 10 ** (-30 / 20))
    out = {}
    for curve in ["A", "B"]:
        a, b = R.both(x, SR, sat=6.0, curve=curve, unity=True)
        out[curve] = dict(ref_db=M.db(M.rms(a[SR // 8:])) - M.db(M.rms(x[SR // 8:])),
                          ours_db=M.db(M.rms(b[SR // 8:])) - M.db(M.rms(x[SR // 8:])))
    return out


def tone_gain(fn, f, L, **s):
    N = 1 << 14
    fc = S.coherent_f(f, SR, N)
    x = S.sine(fc, SR, (N + SR // 4) / SR, 10 ** (L / 20))
    y = fn(x, SR, **s)[SR // 4:]
    w = np.hanning(N)
    k = int(round(fc * N / SR))
    X = np.abs(np.fft.rfft(x[SR // 4:] * w))[k - 2:k + 3].sum()
    Y = np.abs(np.fft.rfft(y * w))[k - 2:k + 3].sum()
    return 20 * np.log10(Y / X)


def t_contour():
    out = {"static_8k": [], "freq": []}
    base = dict(clip=True, mode="Hard")
    for L in [-12, -9, -6, -3, -1, 0]:
        d = [tone_gain(fn, 8000, L, contour=2.0, **base) - tone_gain(fn, 8000, L, **base) for fn in (R.ref, R.ours)]
        out["static_8k"].append([L, *d])
    for f in [1000, 2000, 3000, 4000, 6000, 10000, 15000]:
        d = [tone_gain(fn, f, -0.5, contour=2.0, **base) - tone_gain(fn, f, -0.5, **base) for fn in (R.ref, R.ours)]
        out["freq"].append([f, *d])
    allv = out["static_8k"] + out["freq"]
    out["max_db"] = float(max(abs(r[1] - r[2]) for r in allv))
    return out


def t_linear():
    """Low-level magnitude responses: clip filter (per mode) and tone A/B."""
    x = S.noise(SR, 6.0, -40.0)
    out = {}
    cases = {"filter_A": dict(clip=True, mode="A", filter=True), "filter_B": dict(clip=True, mode="B", filter=True),
             "filter_Hard": dict(clip=True, mode="Hard", filter=True), "tone_A": dict(tone="A"), "tone_B": dict(tone="B")}
    fs_chk = np.array([100, 1000, 3000, 5000, 8000, 10000, 15000, 18000, 20000])
    for k, s in cases.items():
        a, b = R.both(x, SR, **s)
        f, Ha = M.mag_response(x, a, SR)
        _, Hb = M.mag_response(x, b, SR)
        ia = [np.argmin(abs(f - v)) for v in fs_chk]
        da, dbb = M.db(Ha[ia]), M.db(Hb[ia])
        out[k] = dict(f=fs_chk.tolist(), ref=da.tolist(), ours=dbb.tolist(), max_db=float(np.max(np.abs(da - dbb))))
    return out


def t_loudness():
    x = mix(SR, 12.0)
    grid = {
        "clipB_+6": dict(clip=True, mode="B", input=6.0),
        "clipA_+6": dict(clip=True, mode="A", input=6.0),
        "hard_+6": dict(clip=True, mode="Hard", input=6.0),
        "hard_+10": dict(clip=True, mode="Hard", input=10.0),
        "B_+6_satB3": dict(clip=True, mode="B", input=6.0, sat=3.0, curve="B"),
        "A_+6_satA6": dict(clip=True, mode="A", input=6.0, sat=6.0, curve="A"),
        "hard_+6_contour2": dict(clip=True, mode="Hard", input=6.0, contour=2.0),
        "B_+8_all": dict(clip=True, mode="B", input=8.0, sat=4.0, curve="B", contour=1.5, tone="A"),
    }
    out = {}
    for k, s in grid.items():
        a, b = R.both(x, SR, **s)
        bg = R.ours(x, SR, **s, guard=True)
        la, lb, lg = M.lufs(a, SR), M.lufs(b, SR), M.lufs(bg, SR)
        out[k] = dict(ref_lufs=la, ours_lufs=lb, diff_lu=lb - la, ours_guard_lufs=lg, diff_guard_lu=lg - la,
                      ref_peak=M.db(M.peak(a)), ours_peak=M.db(M.peak(b)),
                      ref_tp=M.db(M.true_peak(a[0])), ours_tp=M.db(M.true_peak(b[0])), ours_guard_tp=M.db(M.true_peak(bg[0])))
    out["max_abs_diff_lu"] = float(max(abs(v["diff_lu"]) for v in out.values() if isinstance(v, dict)))
    return out


TESTS = dict(static_clip=t_static_clip, levels=t_levels, saturation=t_saturation, unity=t_unity,
             contour=t_contour, linear=t_linear, loudness=t_loudness)

if __name__ == "__main__":
    sel = sys.argv[1:] or list(TESTS)
    path = "../docs/data/parity.json"
    try:
        res = json.load(open(path))
    except Exception:
        res = {}
    for k in sel:
        res[k] = TESTS[k]()
        print(k, json.dumps(res[k], default=float)[:1500])
    json.dump(res, open(path, "w"), indent=1, default=float)
