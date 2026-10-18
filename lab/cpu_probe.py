"""One CPU probe run (see superiority.cpu).

    cpu_probe.py {ref|ours} QUALITY SECONDS

Loads the plugin, applies the CPU-test settings (Classic clip, Gold 3,
Alchemy 1), primes one second, then renders SECONDS of the synthetic mix in
20 s calls with a 512-sample host buffer. Prints the CPU cycles spent in the
render loop alone, read from the process's own counters (proc_pid_rusage,
rusage_info_v4.ri_cycles, Apple silicon): cycles do not depend on clock
frequency or core type, and plugin load / licence check stay outside.
"""
import ctypes
import os
import sys

import numpy as np
import pedalboard

import goldclip as G
import plugin_check as PC
from music_synth import mix

SR = 48000


class _RusageV4(ctypes.Structure):
    _fields_ = [("uuid", ctypes.c_uint8 * 16)] + [(f"f{i}", ctypes.c_uint64) for i in range(29)] + [
        ("instructions", ctypes.c_uint64), ("cycles", ctypes.c_uint64), ("rest", ctypes.c_uint64 * 4)]


def cycles() -> int:
    r = _RusageV4()
    if ctypes.CDLL("/usr/lib/libproc.dylib").proc_pid_rusage(os.getpid(), 4, ctypes.byref(r)) != 0:
        raise OSError("proc_pid_rusage failed")
    return r.cycles


if __name__ == "__main__":
    which, quality, secs = sys.argv[1], sys.argv[2], float(sys.argv[3])
    x = mix(SR, 20.0).astype(np.float32)
    if which == "ref":
        G.set_params(clipper_on_off=True, clipper_mode="Classic", gold_on_off=True, gold=3.0,
                     alchemy_on_off=True, alchemy=1.0)
        p = G.plugin()
    else:
        p = G._Fixed(pedalboard.load_plugin(os.path.expanduser(PC.PATHS["au"])))
        PC.apply(p, **{**PC.BASE, "clipper_mode": "Classic", "gold_db": 3.0, "alchemy_db": 1.0,
                       "oversampling": {"high": "High", "pristine": "Pristine", "extra": "Extra Pristine"}[quality]})
    p(x[:, :SR], SR)
    c0 = cycles()
    for _ in range(int(secs / 20)):
        p(x, SR, reset=False)
    print(cycles() - c0)
