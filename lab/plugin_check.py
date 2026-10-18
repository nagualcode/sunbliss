"""Integration check: the installed VST3/AU binary must produce the same
audio as the CLI (brief §6.1). Floats are set through exact normalized
values: pedalboard's own float setter snaps to a sampled grid."""
import sys
import numpy as np
import pedalboard
import metrics as M
import render as R
import stimuli as S

PATHS = {"vst3": "~/Library/Audio/Plug-Ins/VST3/Sunbliss.vst3",
         "au": "~/Library/Audio/Plug-Ins/Components/Sunbliss.component"}
RANGES = {"input_trim_db": (-12, 12), "output_trim_db": (-12, 12), "trim_link_offset_db": (-12, 12),
          "clip_ceiling_db": (-36, 0), "gold_db": (0, 6), "alchemy_db": (0, 2), "wet": (0, 1), "dry": (0, 1)}
ENUMS = {"clipper_mode": ["Classic", "Modern", "Hard"], "gold_mode": ["Classic", "Modern"],
         "box_tone": ["Flat", "Classic", "Modern"], "oversampling": ["High", "Pristine", "Extra Pristine"]}


def apply(sb, **kw):
    for k, v in kw.items():
        # VST3 names carry the unit suffix (input_db), AU names do not (input)
        name = k if k in sb.parameters else k.removesuffix("_db")
        p = sb.parameters[name]
        if k in RANGES:
            lo, hi = RANGES[k]
            p.raw_value = (v - lo) / (hi - lo)
        elif k in ENUMS:
            p.raw_value = ENUMS[k].index(v) / (len(ENUMS[k]) - 1)
        else:
            p.raw_value = 1.0 if v else 0.0


BASE = dict(clipper=True, trim_link=False, input_trim_db=0.0, output_trim_db=0.0, gold_db=0.0, alchemy_db=0.0,
            clip_guard=False, box_tone="Flat", wet=1.0, dry=0.0, dry_wet_link=True, clipper_mode="Hard",
            gold_mode="Modern", clip_filter=False, unity_gain=False, delta=False)

CASES = [
    (dict(), dict(clip=True, mode="Hard")),
    (dict(input_trim_db=6.0), dict(clip=True, mode="Hard", input=6.0)),
    (dict(input_trim_db=8.0, clipper_mode="Classic", gold_db=3.0, gold_mode="Classic", alchemy_db=1.0, clip_guard=True),
     dict(clip=True, mode="B", input=8.0, sat=3.0, curve="B", contour=1.0, guard=True)),
    (dict(input_trim_db=6.0, clipper_mode="Modern", box_tone="Modern", wet=0.5), dict(clip=True, mode="A", input=6.0, tone="A", wet=50.0)),
    (dict(input_trim_db=4.0, clipper_mode="Classic", clip_filter=True, gold_db=6.0, gold_mode="Modern", unity_gain=True),
     dict(clip=True, mode="B", input=4.0, filter=True, sat=6.0, curve="A", unity=True)),
]

if __name__ == "__main__":
    import os
    fmt = sys.argv[1] if len(sys.argv) > 1 else "au"
    sb = pedalboard.load_plugin(os.path.expanduser(PATHS[fmt]))
    SR = 48000
    x = np.stack([S.noise(SR, 2.0, -14, "pink", 1), S.noise(SR, 2.0, -14, "pink", 2)])
    worst = -999.0
    for gc, ours in CASES:
        apply(sb, **{**BASE, **gc})
        sb(np.zeros((2, 8192), np.float32), SR)
        sb.reset()
        y = sb(x.astype(np.float32), SR)
        yc = R.ours(x, SR, **ours)
        n = M.null_db(y[:, SR // 2:], yc[:, SR // 2:])
        worst = max(worst, n["peak_dbfs"])
        print(f"{fmt} {ours}: null peak {n['peak_dbfs']:.1f} dBFS")
    print(f"{fmt} worst {worst:.1f} dBFS")
