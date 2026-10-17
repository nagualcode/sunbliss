"""Comparison figures for the README: reference x Sunbliss (x Lavry manual).

Writes docs/fig/readme_*.png. Run from lab/ after `make build`."""
from __future__ import annotations

import json

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

import metrics as M
import render as R
import stimuli as S
from parity import dc_curve

SR = 48000
REF, OURS, INK, GRID = "#eb6834", "#2a78d6", "#52514e", "#e4e3df"
MODES = [("B", "Classic"), ("A", "Modern"), ("Hard", "Hard")]

plt.rcParams.update({
    "axes.edgecolor": INK, "axes.labelcolor": INK, "xtick.color": INK, "ytick.color": INK,
    "axes.grid": True, "grid.color": GRID, "grid.linewidth": 0.8, "axes.spines.top": False,
    "axes.spines.right": False, "font.size": 10, "axes.titlesize": 11, "legend.frameon": False,
})


def clip_curves():
    lv = np.linspace(-1.3, 1.3, 53)
    fig, axs = plt.subplots(1, 3, figsize=(13, 4.2), sharey=True)
    for ax, (m, name) in zip(axs, MODES):
        a, b = dc_curve(R.ref, m, lv), dc_curve(R.ours, m, lv)
        ax.plot(lv, lv, ":", color=INK, lw=1, label="linear")
        ax.plot(lv, a, "o", color=REF, ms=5, label="referência")
        ax.plot(lv, b, "-", color=OURS, lw=2, label="Sunbliss")
        ax.set_title(f"Clipper {name}")
        ax.set_xlabel("entrada (DC, × ceiling)")
    axs[0].set_ylabel("saída (× ceiling)")
    axs[0].legend(loc="upper left")
    fig.suptitle("Curva estática do clipper medida com degraus de DC (48 kHz)", color=INK)
    fig.tight_layout()
    fig.savefig("../docs/fig/readme_clip_parity.png", dpi=110)


def gold_lavry():
    man = json.load(open("../docs/data/lavry_manual_curve_digitized.json"))
    n = SR // 2 + SR // 8
    t = np.arange(n) / SR
    x = 0.999 * np.sin(2 * np.pi * 10 * t)
    s0, per = SR // 8, SR // 40  # rising quarter of the 10 Hz cycle
    fig, axs = plt.subplots(1, 2, figsize=(11, 4.4), sharey=True)
    for ax, g in zip(axs, [3, 6]):
        a, b = R.both(x, SR, sat=float(g), curve="B")
        xs, a, b = x[s0:s0 + per], a[s0:s0 + per], b[s0:s0 + per]
        m = xs > 10 ** (-21 / 20)
        d = np.array(man[f"V{g}"])
        ax.plot([-21, 0], [-21, 0], ":", color=INK, lw=1, label="linear")
        ax.plot(d[:, 0], d[:, 1], ".", color=INK, ms=3, label="manual Lavry (digitalizado)")
        ax.plot(M.db(xs[m]), M.db(a[m]), "-", color=REF, lw=4, alpha=0.55, label="referência (Gold Classic)")
        ax.plot(M.db(xs[m]), M.db(b[m]), "-", color=OURS, lw=1.6, label="Sunbliss (Gold Classic)")
        ax.set_title(f"Soft Saturation +{g} dB")
        ax.set_xlabel("entrada (dBFS)")
        ax.set_xlim(-21, 0.5)
        ax.set_ylim(-21, 0.5)
    axs[0].set_ylabel("saída (dBFS)")
    axs[0].legend(loc="lower right", fontsize=9)
    fig.suptitle("Curva Gold Classic × Apêndice III do manual do AD122-96 MkIII", color=INK)
    fig.tight_layout()
    fig.savefig("../docs/fig/readme_gold_lavry.png", dpi=110)


def alias_spectrum():
    N = 1 << 15
    f0 = S.coherent_f(9000, SR, N)
    x = S.sine(f0, SR, (N + SR // 4) / SR, 10 ** (6 / 20))
    fig, axs = plt.subplots(2, 1, figsize=(11, 6), sharex=True, sharey=True)
    for ax, (fn, name, c) in zip(axs, [(R.ref, "referência", REF), (R.ours, "Sunbliss", OURS)]):
        y = fn(x, SR, clip=True, mode="A")[SR // 4:SR // 4 + N]
        w = np.blackman(N)
        sp = np.abs(np.fft.rfft(y * w))
        f = np.fft.rfftfreq(N, 1 / SR)
        ax.plot(f / 1000, M.db(sp / sp.max()), color=c, lw=0.9)
        ax.set_title(f"{name}: tom de 9 kHz, +6 dB acima do ceiling, clipper Modern")
        ax.set_ylabel("dB rel. fundamental")
        ax.set_ylim(-160, 5)
    axs[1].set_xlabel("frequência (kHz)")
    axs[1].set_xlim(0, 24)
    fig.text(0.5, 0.005, "Os harmônicos de 9 kHz (27, 45 kHz…) caem acima de Nyquist; tudo o que aparece fora de 9 kHz é aliasing ou ruído.",
             ha="center", color=INK, fontsize=9)
    fig.tight_layout(rect=(0, 0.03, 1, 1))
    fig.savefig("../docs/fig/readme_alias_spectrum.png", dpi=110)


def superiority_bars():
    d = json.load(open("../docs/data/superiority.json"))["aliasing"]
    labels, ref, ours = [], [], []
    for m, name in MODES:
        for f in [5000, 9000, 13000, 15000]:
            labels.append(f"{name} {f // 1000} kHz")
            r, o = d[f"{m}_{f}"]
            ref.append(r); ours.append(o)
    y = np.arange(len(labels))[::-1]
    fig, ax = plt.subplots(figsize=(9, 6))
    h = 0.38
    ax.barh(y + h / 2, ref, h - 0.04, color=REF, label="referência")
    ax.barh(y - h / 2, ours, h - 0.04, color=OURS, label="Sunbliss")
    ax.set_yticks(y, labels)
    ax.axvline(0, color=INK, lw=0.8)
    ax.set_xlabel("aliasing + ruído (dB rel. fundamental; menor = melhor)")
    ax.set_title("Tom agudo +6 dB acima do ceiling, 48 kHz, oversampling High", color=INK)
    ax.legend(loc="lower right")
    ax.grid(axis="y", visible=False)
    fig.tight_layout()
    fig.savefig("../docs/fig/readme_aliasing_bars.png", dpi=110)


if __name__ == "__main__":
    clip_curves()
    gold_lavry()
    alias_spectrum()
    superiority_bars()
