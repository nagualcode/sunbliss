"""Superiority measurements (brief §7.2). Writes docs/data/superiority.json."""
from __future__ import annotations

import json
import os
import sys
import time

import numpy as np
import pedalboard

import goldclip as G
import metrics as M
import render as R
import stimuli as S
from music_synth import mix

OUT = "../docs/data/superiority.json"


def aliasing(sr=48000):
    """Hot HF tones driven +6 dB into each clip mode: inharmonic energy
    (aliases + noise) relative to the fundamental."""
    N = 1 << 15
    res = {}
    for mode in ["A", "B", "Hard"]:
        for f in [5000, 9000, 13000, 15000]:
            f0 = S.coherent_f(f, sr, N)
            x = S.sine(f0, sr, (N + sr // 4) / sr, 10 ** (6 / 20))
            a, b = R.both(x, sr, clip=True, mode=mode)
            ra = M.tone_analysis(a[sr // 4:], sr, f0)["inharm_db"]
            rb = M.tone_analysis(b[sr // 4:], sr, f0)["inharm_db"]
            res[f"{mode}_{f}"] = [ra, rb]
    return res


def imd(sr=48000):
    res = {}
    N = 1 << 15
    for mode in ["A", "B", "Hard"]:
        # 19 + 20 kHz, each at -3 dBFS peak sum ~ +3 dB over ceiling
        f1, f2 = S.coherent_f(19000, sr, N), S.coherent_f(20000, sr, N)
        x = S.two_tone(f1, f2, sr, (N + sr // 4) / sr, amp=1.4)
        a, b = R.both(x, sr, clip=True, mode=mode)
        def prod(y):
            y = y[sr // 4:]
            P = np.abs(np.fft.rfft(y * np.blackman(len(y)))) ** 2
            f = np.fft.rfftfreq(len(y), 1 / sr)
            band = lambda c: P[(f > c - 30) & (f < c + 30)].sum()
            ref = band(f1) + band(f2)
            d = f2 - f1
            # everything below 18 kHz is intermodulation / aliasing
            return 10 * np.log10(P[(f > 20) & (f < 18000)].sum() / ref)
        res[f"{mode}_19+20k"] = [prod(a), prod(b)]
    return res


def true_peak_and_loudness(sr=48000):
    x = mix(sr, 12.0)
    res = {}
    for drive in [3.0, 6.0, 10.0]:
        s = dict(clip=True, mode="B", input=drive)
        a = R.ref(x, sr, **s)
        b = R.ours(x, sr, **s, guard=True)
        res[f"B_+{drive:.0f}"] = dict(tp_ref=M.db(max(M.true_peak(a[0]), M.true_peak(a[1]))),
                                    tp_ours=M.db(max(M.true_peak(b[0]), M.true_peak(b[1]))),
                                    lufs_ref=M.lufs(a, sr), lufs_ours=M.lufs(b, sr))
    return res


def transparency(sr=48000):
    """Clipper on, programme 6 dB under the ceiling: residual against the
    input inside the audio band (<= 20 kHz; what each decimator removes
    above that is by design). 'neutral' = everything off, full band."""
    x = mix(sr, 8.0) * 10 ** (-6 / 20)

    def band(d):
        D = np.fft.rfft(d, axis=-1)
        f = np.fft.rfftfreq(d.shape[-1], 1 / sr)
        D[..., f > 20000] = 0
        return np.fft.irfft(D, d.shape[-1], axis=-1)

    res = {}
    for mode in ["A", "B", "Hard"]:
        a, b = R.both(x, sr, clip=True, mode=mode)
        ra = band(a[:, sr:] - x[:, sr:])
        rb = band(b[:, sr:] - x[:, sr:])
        res[mode] = [M.db(M.rms(ra)), M.db(M.rms(rb))]
    a, b = R.both(x, sr)  # everything off
    res["neutral"] = [M.null_db(a[:, sr:], x[:, sr:])["peak_dbfs"], M.null_db(b[:, sr:], x[:, sr:])["peak_dbfs"]]
    return res


def recovery(sr=48000):
    """1 kHz probe at -12 dBFS; 20 ms burst of a steep +12 dB square at 0.5 s.
    Max level error of the probe 50..500 ms after the burst."""
    t = np.arange(int(sr * 1.5)) / sr
    probe = 0.25 * np.sin(2 * np.pi * 1000 * t)
    x = probe.copy()
    i0 = int(0.5 * sr)
    x[i0:i0 + int(0.02 * sr)] = 4.0 * np.sign(np.sin(2 * np.pi * 100 * t[:int(0.02 * sr)]) + 1e-9)
    res = {}
    for mode in ["Hard", "B"]:
        out = []
        for fn in (R.ref, R.ours):
            y = fn(x, sr, clip=True, mode=mode)
            w = 480
            seg = slice(i0 + int(0.07 * sr), i0 + int(0.5 * sr))
            e = [20 * np.log10(M.rms(y[k:k + w]) / M.rms(probe[k:k + w])) for k in range(seg.start, seg.stop - w, w)]
            out.append(float(np.max(np.abs(e))))
        res[mode] = out
    return res


def automation_clicks(sr=48000):
    """Switch a discrete control mid-programme (at a block boundary) and
    measure the *excess* energy above 16 kHz in +-5 ms around the switch:
    HF of the switched render minus the larger HF of the two steady
    renders over the same window (dB). <= 0 means no audible click."""
    from scipy import signal
    import plugin_check as PC
    x = mix(sr, 4.0)[:, :int(2.0 * sr)] * 10 ** (2 / 20)
    half = sr
    bh, ah = signal.butter(4, 16000, "high", fs=sr)
    win = slice(half - int(0.005 * sr), half + int(0.005 * sr))

    def hf(y):
        z = signal.lfilter(bh, ah, y[0])
        return 10 * np.log10(np.mean(z[win] ** 2) + 1e-30)

    def render_gc(p1, p2):
        gc = G.plugin()
        def go(p):
            G.set_params(**p); gc.reset(); gc(np.zeros((2, 40000), np.float32), sr)
        go(p1)
        y1 = gc(x[:, :half].astype(np.float32), sr, reset=False)
        for k, v in p2.items():
            setattr(gc, k, v)
        y2 = gc(x[:, half:].astype(np.float32), sr, reset=False)
        sw = np.hstack([y1, y2])
        go(p1); a = gc(x.astype(np.float32), sr, reset=False)
        go(p2 | {k: v for k, v in p1.items() if k not in p2}); b = gc(x.astype(np.float32), sr, reset=False)
        return hf(sw) - max(hf(a), hf(b))

    sb = G._Fixed(pedalboard.load_plugin(os.path.expanduser("~/Library/Audio/Plug-Ins/VST3/Sunbliss.vst3")))

    def render_sb(p1, p2):
        def go(p):
            PC.apply(sb, **p); sb.reset(); sb(np.zeros((2, 40000), np.float32), sr)
        go(p1)
        y1 = sb(x[:, :half].astype(np.float32), sr, reset=False)
        PC.apply(sb, **p2)
        y2 = sb(x[:, half:].astype(np.float32), sr, reset=False)
        sw = np.hstack([y1, y2])
        go(p1); a = sb(x.astype(np.float32), sr, reset=False)
        go({**p1, **p2}); b = sb(x.astype(np.float32), sr, reset=False)
        return hf(sw) - max(hf(a), hf(b))

    gcb = dict(G.NEUTRAL, clipper_on_off=True)
    sbb = dict(PC.BASE)
    cases = {
        "mode B->Hard": ((gcb | dict(clipper_mode="Classic"), dict(clipper_mode="Hard")),
                         (sbb | dict(clipper_mode="Classic"), dict(clipper_mode="Hard"))),
        "curve A->B (sat 6)": ((gcb | dict(gold_on_off=True, gold=6.0, gold_mode="Modern"), dict(gold_mode="Classic")),
                               (sbb | dict(gold_db=6.0, gold_mode="Modern"), dict(gold_mode="Classic"))),
        "clip on->off": ((gcb, dict(clipper_on_off=False)), (sbb, dict(clipper=False))),
        "tone Flat->A": ((gcb | dict(box_tone_mode="Flat"), dict(box_tone_mode="Modern")),
                         (sbb | dict(box_tone="Flat"), dict(box_tone="Modern"))),
        "bypass on": ((gcb, dict(bypass=True)), (sbb, dict(bypass=True))),
    }
    return {k: [render_gc(*g), render_sb(*o)] for k, (g, o) in cases.items()}


def cpu(sr=48000, secs=100.0, reps=3):
    """CPU cycles per second of stereo audio, same host (pedalboard, buffer
    512): lab/cpu_probe.py reads the process's own cycle counter around the
    render loop (median of `reps` runs). Wall-clock timings of single renders
    swung by tens of percent with clock frequency and core type, and the
    reference's licence check made whole-process counts noisy."""
    import subprocess

    def run(which, q):
        out = subprocess.run([sys.executable, "cpu_probe.py", which, q, str(secs)],
                             capture_output=True, text=True, check=True).stdout
        return float(out.split()[-1]) / secs

    res = {}
    for which, qs in (("ref", ["high"]), ("ours", ["high", "pristine", "extra"])):
        for q in qs:
            runs = sorted(run(which, q) for _ in range(reps))
            res[f"{'reference' if which == 'ref' else 'ours'}_{q}"] = runs[len(runs) // 2]
    res["unit"] = "cycles per second of audio"
    return res


TESTS = dict(aliasing=aliasing, imd=imd, true_peak=true_peak_and_loudness, transparency=transparency,
             recovery=recovery, automation=automation_clicks, cpu=cpu)

if __name__ == "__main__":
    sel = sys.argv[1:] or list(TESTS)
    try:
        res = json.load(open(OUT))
    except Exception:
        res = {}
    for k in sel:
        res[k] = TESTS[k]()
        print(k, json.dumps(res[k], default=float))
    json.dump(res, open(OUT, "w"), indent=1, default=float)
