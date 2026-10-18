"""Classic clipper: dense DC transfer of the reference and a polynomial fit.

DC plateaus from -1.31 to +1.31 x ceiling (0.01 grid), 20 ms each with soft
edges, read 5-10 ms into the step (after the band-limit ringing, before the overload memory
matters). The reference's Classic curve is smooth and slightly asymmetric
inside that range, so an ordinary least-squares polynomial with no constant
term fits it; the degree is picked by the residual. Writes
docs/data/fit_classic_poly.json (coefficients in increasing powers) for
crates/core/src/curves.rs.
"""
import json

import numpy as np

import render as R
from parity import dc_steps

SR = 48000
LIM = 1.31  # the overload memory takes over above ~1.32


def dc_levels(fn, levels, **s):
    seg = SR // 50
    x = dc_steps(levels, seg)
    y = fn(x, SR, clip=True, **s)
    return np.array([np.median(y[i * 2 * seg + seg // 4:i * 2 * seg + seg // 2]) for i in range(len(levels))])


def fit(x, y, deg):
    v = np.vstack([x ** k for k in range(1, deg + 1)]).T
    c, *_ = np.linalg.lstsq(v, y, rcond=None)
    return np.r_[0.0, c]


if __name__ == "__main__":
    lv = np.round(np.arange(-LIM, LIM + 1e-9, 0.01), 4)
    lv = lv[lv != 0.0]
    y = dc_levels(R.ref, lv, mode="B")
    rows = {}
    for deg in (5, 7, 9, 11):
        c = fit(lv, y, deg)
        r = y - np.polyval(c[::-1], lv)
        rows[deg] = dict(coef=c.tolist(), max_abs=float(np.abs(r).max()), rms=float(np.sqrt(np.mean(r ** 2))))
        print(f"degree {deg:2d}: max residual {rows[deg]['max_abs']:.2e}, rms {rows[deg]['rms']:.2e}")
    best = 9
    c = np.array(rows[best]["coef"])
    p = np.polynomial.Polynomial(c)
    lo = np.linspace(-LIM, LIM, 2001)
    print("monotone on fit range:", bool(np.all(p.deriv()(lo[np.abs(lo) < 1.1]) > 0)))
    json.dump(dict(range=LIM, degree=best, coef=c.tolist(), fits=rows,
                   x=lv.tolist(), y=y.tolist()), open("../docs/data/fit_classic_poly.json", "w"), indent=1)
