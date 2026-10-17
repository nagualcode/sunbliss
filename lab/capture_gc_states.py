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


def fingerprint(state, sr=48000):
    """Measure a captured state with every other control neutral:
    latency, pre-peak energy (phase type), level at 23.5 kHz (decimator)."""
    gc = pedalboard.load_plugin(G.GC_PATH)
    gc.raw_state = state
    gc.gain_link = False
    for k, v in G.NEUTRAL.items():
        if k != "gain_link":
            setattr(gc, k, v)
    x = np.zeros((2, G.WARMUP + (1 << 15)), np.float32)
    x[:, G.WARMUP + 4000] = 0.01
    gc.reset()
    h = gc(x, sr)[0, G.WARMUP + 4000:G.WARMUP + 4000 + 4096] / 0.01
    k = int(np.argmax(np.abs(h)))
    pre = float(np.sum(h[:k] ** 2) / np.sum(h ** 2))
    H = np.fft.rfft(h, 1 << 15)
    f = np.fft.rfftfreq(1 << 15, 1 / sr)
    top = float(20 * np.log10(abs(H[np.argmin(abs(f - 23500))])))
    return (k, round(pre, 3), round(top, 1))


def main():
    os.makedirs(OUT, exist_ok=True)
    only = sys.argv[1:]
    seen = {}
    for name, how in MODES:
        if only and name not in only:
            continue
        while True:
            gc = pedalboard.load_plugin(G.GC_PATH)
            print("\n" + "=" * 70)
            print(f"[{name}]  Set:  {how}")
            print("Set BOTH menus (rate and Linear/Minimum), check the window shows them,")
            print("then CLOSE the plugin window. Other controls do not matter.")
            print("=" * 70)
            gc.show_editor()
            state = gc.raw_state
            fp = fingerprint(state)
            print(f"measured: latency {fp[0]} samples, pre-peak energy {fp[1]:.3f} "
                  f"(linear ~0.004, minimum phase should be ~0), level @23.5 kHz {fp[2]} dB")
            dup = [n for n, v in seen.items() if v == fp]
            if dup:
                print(f"!! identical to {dup[0]} - the menu change probably did not register.")
            ans = input("keep this capture? [Y/n] ").strip().lower()
            if ans in ("", "y", "yes", "s", "sim"):
                break
        open(os.path.join(OUT, f"{name}.bin"), "wb").write(state)
        seen[name] = fp
        print(f"saved {name}.bin")
    print("\nDone. Tell Claude the capture finished.")


if __name__ == "__main__":
    main()
