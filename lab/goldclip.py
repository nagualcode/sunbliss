"""Gold Clip (reference) loader and renderer helpers.

The AU reports 0 latency to pedalboard, so we strip the measured latency
ourselves. After `reset()` the first ~10k samples at 48 kHz come out of a
stale configuration (measured: IR peak 0.762 instead of 0.979), so every
render is preceded by `WARMUP` samples of silence that are discarded.

Every render uses a host buffer of `BUFFER` samples. The reference's hidden
output gain ("OversamplingWithCeiling" makeup) decides once per block and
holds for two blocks, so its ducking scales with the buffer: pedalboard's
default (8192) holds ~230 ms where a DAW at 512 holds ~5 ms.
"""
from __future__ import annotations

import numpy as np
import pedalboard

GC_PATH = "/Library/Audio/Plug-Ins/Components/Gold Clip.component"

# Neutral-ish baseline: everything that colours the sound off.
NEUTRAL = dict(
    bypass=False, input_gain=0.0, output_gain=0.0, gain_link=False,
    ceiling=0.0, clipper_on_off=False, clipper_mode="Classic",
    clipper_filter_on_off=False, clip_guard=False,
    gold_on_off=False, gold=0.0, gold_mode="Modern", unity=False,
    alchemy_on_off=False, alchemy=0.0, box_tone_mode="Flat",
    dry_wet_link=True, wet=100.0, dry=0.0, wet_mute=False, dry_mute=False,
    delta_phase=False,
)

_plugin = None
_latency: dict[int, int] = {}
WARMUP = 32768
BUFFER = 512


class _Fixed:
    """Plugin proxy that renders with the lab's host buffer size."""

    def __init__(self, p):
        object.__setattr__(self, "_p", p)

    def __call__(self, x, sr, **kw):
        kw.setdefault("buffer_size", BUFFER)
        return self._p(x, sr, **kw)

    def __getattr__(self, k):
        return getattr(self._p, k)

    def __setattr__(self, k, v):
        setattr(self._p, k, v)


def plugin():
    global _plugin
    if _plugin is None:
        _plugin = _Fixed(pedalboard.load_plugin(GC_PATH))
    return _plugin


def set_params(**kw):
    gc = plugin()
    p = dict(NEUTRAL)
    p.update(kw)
    # gain_link must be set before trims, otherwise moving input drags output.
    gc.gain_link = p.pop("gain_link")
    gc.dry_wet_link = p.pop("dry_wet_link")
    for k, v in p.items():
        setattr(gc, k, v)
    # Read back to verify (AU params are quantised to their steps).
    return {k: getattr(gc, k) for k in kw}


def latency(sr: int) -> int:
    """Latency in samples at `sr`, from the IR peak with everything neutral."""
    if sr not in _latency:
        set_params()
        n = WARMUP + 8192
        x = np.zeros((2, n), np.float32)
        x[:, WARMUP + 1000] = 0.01
        gc = plugin()
        gc.reset()
        y = gc(x, sr)
        idx = int(np.argmax(np.abs(y[0])))
        _latency[sr] = idx - WARMUP - 1000
    return _latency[sr]


def render(x: np.ndarray, sr: int, align: bool = True, **params) -> np.ndarray:
    """Render mono or (ch, n) float array through Gold Clip with `params`.

    Returns an array with the same shape as `x`, latency removed.
    """
    mono = x.ndim == 1
    xs = np.atleast_2d(x).astype(np.float32)
    if xs.shape[0] == 1:
        xs = np.vstack([xs, xs])
    lat = latency(sr) if align else 0
    set_params(**params)
    gc = plugin()
    gc.reset()
    pre = np.zeros((2, WARMUP), np.float32)
    pad = np.zeros((2, lat + 4096), np.float32)
    y = gc(np.hstack([pre, xs, pad]), sr)
    y = y[:, WARMUP + lat:WARMUP + lat + xs.shape[1]].astype(np.float64)
    return y[0] if mono else y
