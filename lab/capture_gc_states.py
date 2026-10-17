"""Interactive capture of the reference's hidden oversampling settings.

Run from the project root in a terminal (it opens the plugin window):

    .venv/bin/python lab/capture_gc_states.py

For each mode it opens the reference editor; set the oversampling options
shown in the prompt, then CLOSE the window. The plugin's own saved state
(an opaque blob, never decoded) is stored in refs/goldclip_states/ and the
mode is verified by measuring the latency it produces.
"""
import os
import sys

import numpy as np
import pedalboard

sys.path.insert(0, os.path.dirname(__file__))
import goldclip as G

OUT = os.path.join(os.path.dirname(__file__), "..", "refs", "goldclip_states")
MODES = [
    ("high_linear", "Oversampling: High  |  Type: Linear Phase"),
    ("pristine_linear", "Oversampling: Pristine  |  Type: Linear Phase"),
    ("extra_linear", "Oversampling: Extra Pristine  |  Type: Linear Phase"),
    ("high_minimum", "Oversampling: High  |  Type: Minimum Phase"),
    ("pristine_minimum", "Oversampling: Pristine  |  Type: Minimum Phase"),
    ("extra_minimum", "Oversampling: Extra Pristine  |  Type: Minimum Phase"),
]


def ir_peak(gc, sr=48000):
    """Latency (samples) and peak value of the impulse response."""
    x = np.zeros((2, G.WARMUP + 16384), np.float32)
    x[:, G.WARMUP + 1000] = 0.01
    gc.reset()
    y = gc(x, sr)[0, G.WARMUP:]
    k = int(np.argmax(np.abs(y)))
    return k - 1000, float(y[k] / 0.01)


def main():
    os.makedirs(OUT, exist_ok=True)
    only = sys.argv[1:]
    for name, how in MODES:
        if only and name not in only:
            continue
        gc = pedalboard.load_plugin(G.GC_PATH)
        print("\n" + "=" * 70)
        print(f"[{name}]  Set:  {how}")
        print("Everything else may stay as it is. Then CLOSE the plugin window.")
        print("(the oversampling menu is at the bottom-left of the window)")
        print("=" * 70)
        gc.show_editor()
        state = gc.raw_state
        open(os.path.join(OUT, f"{name}.bin"), "wb").write(state)
        # verify through a fresh instance restored from the blob
        gc2 = pedalboard.load_plugin(G.GC_PATH)
        gc2.raw_state = state
        lat, pk = ir_peak(gc2)
        print(f"saved {name}.bin  ({len(state)} bytes)  ->  latency {lat} samples @48k, IR peak {pk:.3f}")
    print("\nDone. Tell Claude the capture finished.")


if __name__ == "__main__":
    main()
