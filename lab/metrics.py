"""Measurement primitives shared by probes and the report."""
from __future__ import annotations

import numpy as np
from scipy import signal


def db(x, floor=1e-20):
    return 20 * np.log10(np.maximum(np.abs(x), floor))


def rms(x):
    return float(np.sqrt(np.mean(np.square(x))))


def peak(x):
    return float(np.max(np.abs(x)))


def true_peak(x, os=8, seg=1 << 15):
    """Inter-sample peak of the ideal band-limited reconstruction.

    FFT (sinc) interpolation by `os` on overlapping segments; segment edges
    are discarded, so the result does not depend on an interpolation
    filter's passband/transition choice (scipy's polyphase meters differ
    by up to 0.4 dB on clipped noise)."""
    x = np.asarray(x, float)
    hop = seg // 2
    best = 0.0
    for start in range(0, max(len(x) - seg, 0) + 1, hop):
        blk = x[start:start + seg]
        if len(blk) < 64:
            break
        w = signal.windows.tukey(len(blk), 0.25)
        y = signal.resample(blk * w, len(blk) * os)
        q = len(y) // 8  # keep the flat-window middle
        best = max(best, float(np.max(np.abs(y[q:-q]))))
    if len(x) <= seg:
        return max(best, float(np.max(np.abs(x))))
    return best


def _win_spectrum(x):
    w = signal.windows.blackmanharris(len(x))
    X = np.fft.rfft(x * w)
    return np.abs(X) ** 2, w


def tone_analysis(x, sr, f0, nharm=None, bw_bins=6):
    """Split the power spectrum of a steady tone into fundamental, harmonics
    (true multiples < Nyquist) and everything else (aliases + noise).

    Returns dict with dB-relative-to-fundamental figures.
    """
    n = len(x)
    P, _ = _win_spectrum(x)
    df = sr / n
    used = np.zeros(len(P), bool)

    def band(f):
        k = int(round(f / df))
        lo, hi = max(k - bw_bins, 0), min(k + bw_bins + 1, len(P))
        return lo, hi

    lo, hi = band(f0)
    p_fund = P[lo:hi].sum(); used[lo:hi] = True
    p_harm = 0.0
    h = 2
    while h * f0 < sr / 2 and (nharm is None or h <= nharm):
        lo, hi = band(h * f0)
        p_harm += P[lo:hi].sum(); used[lo:hi] = True
        h += 1
    used[:3] = True  # DC
    p_other = P[~used].sum()
    return dict(
        fund_db=float(10 * np.log10(p_fund + 1e-30)),
        thd_db=float(10 * np.log10(p_harm / p_fund + 1e-30)),
        inharm_db=float(10 * np.log10(p_other / p_fund + 1e-30)),
    )


def aliasing_db(x, sr, f0, bw_bins=6):
    """Energy not on the harmonic grid of f0, relative to the fundamental.
    With a pure-tone input this is aliasing + noise (inharmonic energy)."""
    return tone_analysis(x, sr, f0, bw_bins=bw_bins)["inharm_db"]


def lufs(x, sr):
    import pyloudnorm as pyln
    meter = pyln.Meter(sr)
    xs = x.T if x.ndim == 2 else x
    return float(meter.integrated_loudness(xs))


def null_db(a, b):
    """Residual of a-b relative to full scale, dBFS (peak) and rms."""
    d = np.asarray(a) - np.asarray(b)
    return dict(peak_dbfs=float(db(peak(d))), rms_dbfs=float(db(rms(d))))


def mag_response(x, y, sr, nfft=None):
    """Transfer function magnitude/phase from broadband x -> y (H1)."""
    nfft = nfft or 1 << 14
    f, pxy = signal.csd(x, y, sr, nperseg=nfft)
    _, pxx = signal.welch(x, sr, nperseg=nfft)
    H = pxy / pxx
    return f, H
