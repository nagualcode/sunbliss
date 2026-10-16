"""Gold (saturation) static curves of the reference.

Clipper off, 40 Hz sine at 0.999 peak so the whole [0, 1] range is
traced without clipping; the rising and falling quarter-periods are
compared to check the block is memoryless.
"""
import json
import numpy as np
import goldclip as G

SR = 48000
F = 10.0


def trace(ceiling=0.0, peak=0.999, **p):
    n = SR // 2 + SR // 8
    t = np.arange(n) / SR
    a = peak * 10 ** (ceiling / 20)
    x = a * np.sin(2 * np.pi * F * t)
    y = G.render(x, SR, ceiling=ceiling, **p)
    s = SR // 8
    per = int(SR / F)
    # The reference has a sub-sample group-delay offset; find the fractional
    # delay d that best explains y as a memoryless function of x(t - d).
    tt = t[s:s + per]
    best = None
    for d in np.linspace(-1.5, 1.5, 61) / SR:
        xd = a * np.sin(2 * np.pi * F * (tt - d))
        _, hyst, _ = curve_from(xd, y[s:s + per], np.linspace(0.01, 0.99 * a, 200))
        if best is None or hyst < best[0]:
            best = (hyst, d, xd)
    trace.delay = best[1] * SR
    return best[2], y[s:s + per]


def curve_from(x, y, grid):
    up = np.gradient(x) > 0
    pos = x > 0
    xr, yr = x[pos & up], y[pos & up]
    xf, yf = x[pos & ~up], y[pos & ~up]
    o = np.argsort(xr); xr, yr = xr[o], yr[o]
    o = np.argsort(xf); xf, yf = xf[o], yf[o]
    yr_g = np.interp(grid, xr, yr); yf_g = np.interp(grid, xf, yf)
    xn, yn = -x[x < 0], -y[x < 0]
    o = np.argsort(xn)
    yn_g = np.interp(grid, xn[o], yn[o])
    return yr_g, float(np.max(np.abs(yr_g - yf_g))), float(np.max(np.abs(yr_g - yn_g)))


if __name__ == "__main__":
    grid = np.linspace(0.002, 0.998, 500)
    out = {"grid": grid.tolist()}
    for mode in ["Classic", "Modern"]:
        for g in np.arange(0.5, 6.01, 0.5):
            x, y = trace(clipper_on_off=False, gold_on_off=True, gold=float(g), gold_mode=mode)
            yc, hyst, odd = curve_from(x, y, grid)
            small = yc[:20] / grid[:20]
            gain_db = 20 * np.log10(np.median(small))
            out[f"{mode}_{g:.1f}"] = yc.tolist()
            print(f"d={trace.delay:+.2f} {mode} {g:.1f}: small-signal {gain_db:+.3f} dB  y(0.998)={yc[-1]:.4f}  hyst={hyst:.1e} odd={odd:.1e}")
    # ceiling dependence: Classic 6 at ceiling -6 dB, normalised
    x, y = trace(ceiling=-6.0, clipper_on_off=True, clipper_mode="Hard", gold_on_off=True, gold=6.0, gold_mode="Classic")
    c = 10 ** (-6 / 20)
    yc, _, _ = curve_from(x / c, y / c, grid)
    print("Classic 6 @ceiling -6, normalised vs @0: max diff", np.max(np.abs(yc - np.array(out['Classic_6.0']))))
    # Unity
    x, y = trace(clipper_on_off=False, gold_on_off=True, gold=6.0, gold_mode="Classic", unity=True)
    yc, _, _ = curve_from(x, y, grid)
    print("Classic 6 unity: small gain", 20*np.log10(np.median(yc[:20]/grid[:20])), "y(.998)", yc[-1])
    out["Classic_6.0_unity"] = yc.tolist()
    x, y = trace(clipper_on_off=False, gold_on_off=True, gold=6.0, gold_mode="Modern", unity=True)
    yc, _, _ = curve_from(x, y, grid)
    print("Modern 6 unity: small gain", 20*np.log10(np.median(yc[:20]/grid[:20])), "y(.998)", yc[-1])
    out["Modern_6.0_unity"] = yc.tolist()
    json.dump(out, open("../docs/data/gc_gold_curves.json", "w"))
