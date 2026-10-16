"""Static transfer curves of the reference, from a slow sine.

A 50 Hz sine at peak `A` drives the block; plotting output sample vs input
sample over one period traces y = f(x). Harmonics up to 20 kHz survive the
reference's band limit, so the curve shape is preserved except at sharp
corners (Hard mode), where Gibbs ringing appears.
"""
import json, sys
import numpy as np
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import goldclip as G

SR = 48000


def curve(peak_db=12.0, f=50.0, **params):
    n = SR // 2
    t = np.arange(n) / SR
    x = 10 ** (peak_db / 20) * np.sin(2 * np.pi * f * t)
    y = G.render(x, SR, **params)
    sl = slice(SR // 10, SR // 10 + int(SR / f))  # one period, steady state
    return x[sl], y[sl]


def binned(x, y, nb=400):
    """Average y in bins of x>0 (odd symmetry assumed and checked)."""
    m = x > 1e-4
    xb = np.linspace(0, x.max(), nb + 1)
    idx = np.digitize(x[m], xb)
    xs, ys = [], []
    for i in range(1, nb + 1):
        s = idx == i
        if s.any():
            xs.append(x[m][s].mean()); ys.append(y[m][s].mean())
    return np.array(xs), np.array(ys)


if __name__ == "__main__":
    out = {}
    fig, axs = plt.subplots(1, 2, figsize=(13, 5.5))
    for mode in ["Hard", "Classic", "Modern"]:
        x, y = curve(12.0, clipper_on_off=True, clipper_mode=mode)
        xb, yb = binned(x, y)
        out[f"clip_{mode}"] = [xb.tolist(), yb.tolist()]
        axs[0].plot(xb, yb, label=mode)
        # odd-symmetry check
        xn, yn = binned(-x, -y)
        print(mode, "odd-sym max err", np.max(np.abs(np.interp(xb, xn, yn) - yb)))
    axs[0].set_title("Clipper (ceiling 0 dBFS), 50 Hz sine @ +12 dB"); axs[0].grid(); axs[0].legend()
    axs[0].set_xlabel("input (lin)"); axs[0].set_ylabel("output (lin)")
    for gmode in ["Classic", "Modern"]:
        for g in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]:
            x, y = curve(0.0, clipper_on_off=True, clipper_mode="Hard",
                         gold_on_off=True, gold=g, gold_mode=gmode)
            xb, yb = binned(x, y)
            out[f"gold_{gmode}_{g}"] = [xb.tolist(), yb.tolist()]
            m = xb > 10 ** (-30 / 20)
            axs[1].plot(20 * np.log10(xb[m]), 20 * np.log10(yb[m]),
                        ls="-" if gmode == "Classic" else "--", label=f"{gmode} {g}")
    axs[1].plot([-30, 0], [-30, 0], "k:", lw=0.8)
    axs[1].set_title("Gold (Hard clip), sine @ 0 dBFS"); axs[1].grid(); axs[1].legend(fontsize=7, ncol=2)
    axs[1].set_xlabel("input dBFS"); axs[1].set_ylabel("output dBFS")
    fig.tight_layout(); fig.savefig("../docs/fig/gc_static_curves.png", dpi=90)
    json.dump(out, open("out/gc_static.json", "w"))
