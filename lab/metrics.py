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


def true_peak(x, os=8):
    """Inter-sample peak by polyphase upsampling (BS.1770-style, 8x)."""
    return float(np.max(np.abs(signal.resample_poly(x, os, 1, window=("kaiser", 12.0)))))


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
