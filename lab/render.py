"""Render the reference and Sunbliss with equivalent settings.

Settings are described once in a neutral vocabulary and mapped to each
plugin, so every comparison uses the same grid:

    clip: bool          mode: "A" | "B" | "Hard"    filter: bool
    ceiling: dB         input: dB                    guard: bool
    sat: 0..6           curve: "A" | "B"             unity: bool
    contour: 0..2       tone: "Flat" | "A" | "B"
    wet, dry: 0..100    mix_link: bool               delta: bool
    quality: "high" | "pristine" | "extra"

A = "Modern" in the reference, B = "Classic".
"""
from __future__ import annotations

import os
import subprocess
import tempfile

import numpy as np

import goldclip as G

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CLI = os.path.join(ROOT, "target", "release", "sunbliss")

DEFAULT = dict(clip=False, mode="B", filter=False, ceiling=0.0, input=0.0, output=0.0, guard=False,
               sat=0.0, curve="A", unity=False, contour=0.0, tone="Flat",
               wet=100.0, dry=0.0, mix_link=True, delta=False, quality="high")

_GC_MODE = {"A": "Modern", "B": "Classic", "Hard": "Hard"}
_GC_TONE = {"Flat": "Flat", "A": "Modern", "B": "Classic"}


def to_gc(s: dict) -> dict:
    s = {**DEFAULT, **s}
    return dict(
        clipper_on_off=s["clip"], clipper_mode=_GC_MODE[s["mode"]], clipper_filter_on_off=s["filter"],
        ceiling=float(s["ceiling"]), input_gain=float(s["input"]), output_gain=float(s["output"]),
        clip_guard=s["guard"], gold_on_off=s["sat"] > 0, gold=float(s["sat"]), gold_mode=_GC_MODE[s["curve"]],
        unity=s["unity"], alchemy_on_off=s["contour"] > 0, alchemy=float(s["contour"]), box_tone_mode=_GC_TONE[s["tone"]],
        dry_wet_link=s["mix_link"], wet=float(s["wet"]), dry=float(s["dry"]), delta_phase=s["delta"],
    )


def to_ours(s: dict) -> dict:
    s = {**DEFAULT, **s}
    return dict(
        neutral=True, link=False, clip_on=s["clip"], clip_mode=s["mode"].lower(), clip_filter=s["filter"],
        ceiling=s["ceiling"], input=s["input"], output=s["output"], guard=s["guard"],
        sat_on=s["sat"] > 0, sat=s["sat"], curve=s["curve"].lower(), unity=s["unity"],
        contour_on=s["contour"] > 0, contour=s["contour"], tone=s["tone"].lower(),
        wet=s["wet"], dry=s["dry"], mix_link=s["mix_link"], delta=s["delta"], quality=s["quality"],
    )


def ours(x: np.ndarray, sr: int, block: int = 512, **s) -> np.ndarray:
    """Render through the Sunbliss CLI (f64 end to end, latency removed)."""
    mono = x.ndim == 1
    xs = np.atleast_2d(x)
    if xs.shape[0] == 1:
        xs = np.vstack([xs, xs])
    import json
    with tempfile.TemporaryDirectory() as d:
        fi, fo, fp = (os.path.join(d, n) for n in ("in.f64", "out.f64", "p.json"))
        xs.T.astype("<f8").tofile(fi)
        json.dump(to_ours(s), open(fp, "w"))
        subprocess.run([CLI, fi, fo, "--rate", str(sr), "--channels", "2", "--compensate", "--block", str(block),
                        "--params", fp], check=True, capture_output=True)
        y = np.fromfile(fo, "<f8").reshape(-1, 2).T
    return y[0] if mono else y


def ref(x: np.ndarray, sr: int, **s) -> np.ndarray:
    """Render through the reference with equivalent settings."""
    return G.render(x, sr, **to_gc(s))


def both(x, sr, **s):
    return ref(x, sr, **s), ours(x, sr, **s)
