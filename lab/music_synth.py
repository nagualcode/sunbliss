"""Deterministic synthetic 'mix' for loudness/clipping tests until the user
provides real material in refs/music/. Not a substitute for listening."""
import numpy as np
from scipy import signal


def mix(sr=48000, dur=16.0, bpm=124, seed=3):
    rng = np.random.default_rng(seed)
    n = int(sr * dur)
    t = np.arange(n) / sr
    beat = 60 / bpm
    L = np.zeros(n); R = np.zeros(n)

    def add(sig, at, pan=0.0, gain=1.0):
        i = int(at * sr)
        m = min(len(sig), n - i)
        if m <= 0:
            return
        L[i:i + m] += sig[:m] * gain * np.sqrt(0.5 * (1 - pan))
        R[i:i + m] += sig[:m] * gain * np.sqrt(0.5 * (1 + pan))

    tk = np.arange(int(0.45 * sr)) / sr
    f = 48 + 90 * np.exp(-tk * 30)
    kick = np.sin(2 * np.pi * np.cumsum(f) / sr) * np.exp(-tk * 7) + 0.3 * rng.standard_normal(len(tk)) * np.exp(-tk * 300)
    ts = np.arange(int(0.25 * sr)) / sr
    b, a = signal.butter(2, [1500, 9000], "band", fs=sr)
    snare = 0.6 * np.sin(2 * np.pi * 190 * ts) * np.exp(-ts * 25) + signal.lfilter(b, a, rng.standard_normal(len(ts))) * np.exp(-ts * 18) * 1.6
    th = np.arange(int(0.06 * sr)) / sr
    bh, ah = signal.butter(4, 7000, "high", fs=sr)
    hat = signal.lfilter(bh, ah, rng.standard_normal(len(th))) * np.exp(-th * 60)
    nb = int(dur / beat)
    for k in range(nb):
        add(kick, k * beat, 0.0, 0.9)
        if k % 2 == 1:
            add(snare, k * beat, 0.05, 0.55)
        for h in range(2):
            add(hat, k * beat + h * beat / 2, 0.4 if h else -0.3, 0.25)
    # bass line (filtered saw)
    notes = [41.2, 41.2, 49.0, 36.7]
    bass = np.zeros(n)
    for k in range(nb):
        f0 = notes[(k // 4) % 4]
        i0, i1 = int(k * beat * sr), int((k + 0.9) * beat * sr)
        tt = np.arange(i1 - i0) / sr
        saw = 2 * ((f0 * tt) % 1) - 1
        bass[i0:i1] += saw * np.minimum(1, tt * 200) * np.exp(-tt * 1.5)
    bl, al = signal.butter(2, 400, fs=sr)
    bass = signal.lfilter(bl, al, bass) * 0.45
    L += bass; R += bass
    # pad
    pad = np.zeros(n)
    for f0 in [220.0, 277.2, 329.6, 415.3]:
        pad += 2 * ((f0 * (1 + 0.002 * rng.standard_normal()) * t) % 1) - 1
    bp, ap = signal.butter(2, 2500, fs=sr)
    pad = signal.lfilter(bp, ap, pad) * 0.05
    L += pad; R += np.roll(pad, int(0.011 * sr))
    x = np.vstack([L, R])
    return x / np.max(np.abs(x)) * 10 ** (-1 / 20)


if __name__ == "__main__":
    import soundfile as sf
    x = mix()
    sf.write("out/synth_mix.wav", x.T, 48000, subtype="FLOAT")
    import metrics as M
    print("LUFS", M.lufs(x, 48000), "peak", M.db(M.peak(x)))
