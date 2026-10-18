"""Oversampling modes: reference (captured GUI states) x Sunbliss, 48 kHz.

Per mode: latency (IR peak), magnitude near the band edge, and inharmonic
energy (aliasing + noise) of a Hard-clipped HF tone at +6 dB. Reference
states are the opaque blobs in refs/goldclip_states/ (never decoded).
Writes docs/data/os_modes.json.
"""
import json
import os
import subprocess

import numpy as np

import goldclip as G
import metrics as M
import render as R
import stimuli as S

SR = 48000
STATES = os.path.join(os.path.dirname(__file__), "..", "refs", "goldclip_states")
REF_MODES = ["high_linear", "pristine_linear", "extra_linear", "high_minimum", "pristine_minimum"]
OURS = [("high", False), ("high", True), ("pristine", False), ("pristine", True), ("extra", True)]
FREQS = [100, 1000, 5000, 10000, 15000, 18000, 20000, 21000, 22000, 23000, 23500]
TONES = [9000, 13000, 15000]


def ref_render(state):
    gc = G.plugin()

    def run(x, **params):
        gc.raw_state = state
        G.set_params(**params)
        gc.reset()
        xs = np.vstack([np.r_[np.zeros(G.WARMUP), x]] * 2).astype(np.float32)
        return gc(xs, SR)[0, G.WARMUP:].astype(float)
    return run


def ours_render(quality, min_phase):
    def run(x, clip=False):
        return R.ours(x, SR, clip=clip, mode="Hard", quality=quality, min_phase=min_phase)
    return run


def measure(run_ir, run_tone, lat=None):
    h = run_ir()
    k = int(np.argmax(np.abs(h))) if lat is None else lat
    H = np.fft.rfft(h[max(k - 4000, 0):k + 4000], 1 << 15)
    f = np.fft.rfftfreq(1 << 15, 1 / SR)
    mag = [round(float(20 * np.log10(abs(H[np.argmin(abs(f - q))]))), 2) for q in FREQS]
    alias = {}
    N = 1 << 15
    for t in TONES:
        f0 = S.coherent_f(t, SR, N)
        x = S.sine(f0, SR, (N + SR // 4) / SR, 10 ** (6 / 20))
        y = run_tone(x)
        y = y[len(y) - N:]
        alias[str(t)] = round(M.tone_analysis(y, SR, f0)["inharm_db"], 1)
    return dict(latency=k, mag_db=mag, alias_hard=alias)


if __name__ == "__main__":
    imp = np.zeros(1 << 14)
    imp[1000] = 1.0
    out = dict(freqs=FREQS, ref={}, ours={})
    for name in REF_MODES:
        st = open(os.path.join(STATES, name + ".bin"), "rb").read()
        run = ref_render(st)
        r = measure(lambda: run(imp * 0.01) / 0.01,
                    lambda x: run(x, clipper_on_off=True, clipper_mode="Hard"))
        r["latency"] -= 1000
        out["ref"][name] = r
        print("ref ", name, r)
    for q, mp in OURS:
        run = ours_render(q, mp)
        r = measure(lambda: run(imp), lambda x: run(x, clip=True))
        r["latency"] = int(subprocess.run([R.CLI, "--latency", "--rate", str(SR), "--set", f"quality={q}",
                                           "--set", f"min_phase={'true' if mp else 'false'}"],
                                          capture_output=True, text=True, check=True).stdout)
        name = f"{q}_{'minimum' if mp else 'linear'}"
        out["ours"][name] = r
        print("ours", name, r)
    json.dump(out, open("../docs/data/os_modes.json", "w"), indent=1)
