//! Static transfer curves.
//!
//! Clip curves are tabulated together with their antiderivative so the
//! clipper can run first-order antiderivative anti-aliasing (ADAA) for any
//! shape. Saturation curves are closed-form (they are C1 and only act below
//! the ceiling, so they alias far less than the clip corner).

/// A memoryless curve f(u), tabulated on [-R, R] with its antiderivative F.
/// Outside the table f is held constant (all clip curves saturate before R).
pub struct TabCurve {
    r: f64,
    inv_du: f64,
    du: f64,
    f: Vec<f64>,
    big_f: Vec<f64>,
}

impl TabCurve {
    pub fn new(r: f64, n: usize, func: impl Fn(f64) -> f64) -> Self {
        let du = 2.0 * r / n as f64;
        let f: Vec<f64> = (0..=n).map(|i| func(-r + i as f64 * du)).collect();
        // Integrate with Simpson on each cell using the true function.
        let mut big_f = vec![0.0; n + 1];
        for i in 0..n {
            let a = -r + i as f64 * du;
            let m = func(a + 0.5 * du);
            big_f[i + 1] = big_f[i] + du / 6.0 * (f[i] + 4.0 * m + f[i + 1]);
        }
        // anchor F(0) = 0 (keeps magnitudes small around the origin)
        let mid = big_f[n / 2];
        big_f.iter_mut().for_each(|v| *v -= mid);
        TabCurve { r, inv_du: 1.0 / du, du, f, big_f }
    }

    #[inline]
    fn cell(&self, u: f64) -> (usize, f64) {
        let p = (u + self.r) * self.inv_du;
        let i = (p as usize).min(self.f.len() - 2);
        (i, p - i as f64)
    }

    /// f(u), linear interpolation.
    #[inline]
    pub fn f(&self, u: f64) -> f64 {
        if u >= self.r {
            return self.f[self.f.len() - 1];
        }
        if u <= -self.r {
            return self.f[0];
        }
        let (i, t) = self.cell(u);
        self.f[i] + t * (self.f[i + 1] - self.f[i])
    }

    /// F(u): cubic Hermite on (F, f) nodes; linear continuation outside.
    #[inline]
    pub fn big_f(&self, u: f64) -> f64 {
        let n = self.f.len() - 1;
        if u >= self.r {
            return self.big_f[n] + self.f[n] * (u - self.r);
        }
        if u <= -self.r {
            return self.big_f[0] + self.f[0] * (u + self.r);
        }
        let (i, t) = self.cell(u);
        let (p0, p1) = (self.big_f[i], self.big_f[i + 1]);
        let (m0, m1) = (self.f[i] * self.du, self.f[i + 1] * self.du);
        let t2 = t * t;
        let t3 = t2 * t;
        (2.0 * t3 - 3.0 * t2 + 1.0) * p0 + (t3 - 2.0 * t2 + t) * m0 + (-2.0 * t3 + 3.0 * t2) * p1 + (t3 - t2) * m1
    }

    /// First-order ADAA between consecutive inputs u0 -> u1.
    #[inline]
    pub fn adaa(&self, u0: f64, u1: f64) -> f64 {
        let d = u1 - u0;
        if d.abs() > 1e-7 {
            (self.big_f(u1) - self.big_f(u0)) / d
        } else {
            self.f(0.5 * (u0 + u1))
        }
    }
}

// ---------------------------------------------------------------- clip curves

/// Hard clip level (fraction of the ceiling). The reference's hard mode
/// settles its clipped plateau slightly below the ceiling (0.97-0.99) so that
/// band-limiting ripple stays under it; we use a fixed 0.99.
pub const HARD_LEVEL: f64 = 0.99;

pub fn hard(u: f64) -> f64 {
    u.clamp(-HARD_LEVEL, HARD_LEVEL)
}

/// Converter A: linear up to `KNEE_A`, C1 quadratic knee into the plateau
/// `LEVEL_A` (measured DC plateau of the "modern" model: 0.9798).
pub const LEVEL_A: f64 = 0.9798;
pub const KNEE_A: f64 = 0.94;

pub fn conv_a(u: f64) -> f64 {
    let a = u.abs();
    let b = 2.0 * LEVEL_A - KNEE_A;
    let y = if a <= KNEE_A {
        a
    } else if a < b {
        a - (a - KNEE_A) * (a - KNEE_A) / (2.0 * (b - KNEE_A))
    } else {
        LEVEL_A
    };
    y.copysign(u)
}

/// Converter B: static curve measured from the "classic" model with DC
/// steps (docs/data/gc_dc_Classic.json), split into odd and even parts.
/// Odd part: gentle compression from low level into a ~0.966 plateau.
/// Even part: small asymmetry (2nd-harmonic source of the analog stage).
const B_U: [f64; 27] = [
    0.0, 0.05, 0.1, 0.15, 0.2, 0.25, 0.3, 0.35, 0.4, 0.45, 0.5, 0.55, 0.6, 0.65, 0.7, 0.75, 0.8, 0.85, 0.9, 0.95,
    1.0, 1.05, 1.1, 1.15, 1.2, 1.25, 1.3,
];

/// Filled from measurement data at build time of the table (see `conv_b_tables`).
pub struct ConvB {
    odd: Vec<f64>,
    even: Vec<f64>,
}

impl ConvB {
    pub fn from_measured(xs: &[f64], ys: &[f64]) -> Self {
        // sample measured curve (both polarities) at |u| grid
        let interp = |x: f64| -> f64 {
            if x <= xs[0] {
                return ys[0];
            }
            for i in 0..xs.len() - 1 {
                if x <= xs[i + 1] {
                    let t = (x - xs[i]) / (xs[i + 1] - xs[i]);
                    return ys[i] + t * (ys[i + 1] - ys[i]);
                }
            }
            ys[ys.len() - 1]
        };
        let mut odd: Vec<f64> = B_U.iter().map(|&u| 0.5 * (interp(u) - interp(-u))).collect();
        let even: Vec<f64> = B_U.iter().map(|&u| 0.5 * (interp(u) + interp(-u))).collect();
        // enforce monotone odd part (measurement noise near the plateau)
        for i in 1..odd.len() {
            if odd[i] < odd[i - 1] {
                odd[i] = odd[i - 1];
            }
        }
        odd[0] = 0.0;
        ConvB { odd, even }
    }

    fn pchip(ys: &[f64], u: f64) -> f64 {
        let h = 0.05;
        let n = ys.len();
        if u >= B_U[n - 1] {
            return ys[n - 1];
        }
        let i = ((u / h) as usize).min(n - 2);
        let t = (u - B_U[i]) / h;
        let d = |k: usize| -> f64 {
            // Fritsch-Carlson slopes
            if k == 0 {
                (ys[1] - ys[0]) / h
            } else if k == n - 1 {
                0.0
            } else {
                let a = (ys[k] - ys[k - 1]) / h;
                let b = (ys[k + 1] - ys[k]) / h;
                if a * b <= 0.0 { 0.0 } else { 2.0 / (1.0 / a + 1.0 / b) }
            }
        };
        let (y0, y1, m0, m1) = (ys[i], ys[i + 1], d(i) * h, d(i + 1) * h);
        let t2 = t * t;
        let t3 = t2 * t;
        (2.0 * t3 - 3.0 * t2 + 1.0) * y0 + (t3 - 2.0 * t2 + t) * m0 + (-2.0 * t3 + 3.0 * t2) * y1 + (t3 - t2) * m1
    }

    pub fn eval(&self, u: f64) -> f64 {
        let a = u.abs();
        Self::pchip(&self.odd, a).copysign(u) + Self::pchip(&self.even, a)
    }
}

/// Default Converter B measurement (x, y) pairs, ceiling = 1.
pub const B_MEAS_X: [f64; 53] = [
    -1.3, -1.25, -1.2, -1.15, -1.1, -1.05, -1.0, -0.95, -0.9, -0.85, -0.8, -0.75, -0.7, -0.65, -0.6, -0.55, -0.5, -0.45,
    -0.4, -0.35, -0.3, -0.25, -0.2, -0.15, -0.1, -0.05, 0.0, 0.05, 0.1, 0.15, 0.2, 0.25, 0.3, 0.35, 0.4, 0.45, 0.5, 0.55,
    0.6, 0.65, 0.7, 0.75, 0.8, 0.85, 0.9, 0.95, 1.0, 1.05, 1.1, 1.15, 1.2, 1.25, 1.3,
];

pub const B_MEAS_Y: [f64; 53] = [
    -0.96352, -0.95000, -0.95320, -0.95990, -0.96219, -0.95601, -0.93993, -0.91419, -0.88006, -0.83924, -0.79357, -0.74474, -0.69419, -0.64300, -0.59191, -0.54136, -0.49149, -0.44228, -0.39355, -0.34507, -0.29661, -0.24795, -0.19897, -0.14962, -0.09992, -0.05000, -0.00000, 0.04988, 0.09946, 0.14859, 0.19719, 0.24527, 0.29292, 0.34033, 0.38776, 0.43552, 0.48391, 0.53317, 0.58341, 0.63452, 0.68614, 0.73757, 0.78771, 0.83515, 0.87813, 0.91472, 0.94307, 0.96171, 0.97008, 0.96922, 0.96270, 0.95778, 0.96688,
];

// ----------------------------------------------------------- saturation curves

/// Curve B (manual): gain g = 10^(G/20) below T = 10^(-2G/20), exponential
/// "tape magnetisation" above, C1 at T, closure y(1) = 1.
pub struct SatB {
    /// A(G) on a 0.01 dB grid, G in [0, 6]
    a_tab: Vec<f64>,
}

impl SatB {
    pub fn new() -> Self {
        let a_tab = (0..=600)
            .map(|i| {
                let gdb = i as f64 * 0.01;
                if gdb < 1e-9 {
                    return 0.0;
                }
                let g = 10f64.powf(gdb / 20.0);
                let t = 10f64.powf(-gdb / 10.0);
                let yt = g * t;
                let h = |a: f64| yt + a * (1.0 - (-(1.0 - t) * g / a).exp()) - 1.0;
                // h increases with a; bisection
                let (mut lo, mut hi) = (1e-12, 10.0);
                for _ in 0..200 {
                    let m = 0.5 * (lo + hi);
                    if h(m) > 0.0 { hi = m } else { lo = m }
                }
                0.5 * (lo + hi)
            })
            .collect();
        SatB { a_tab }
    }

    pub fn coeffs(&self, gdb: f64) -> SatBCoeffs {
        let gdb = gdb.clamp(0.0, 6.0);
        let p = gdb * 100.0;
        let i = (p as usize).min(599);
        let t = p - i as f64;
        let a = self.a_tab[i] + t * (self.a_tab[i + 1] - self.a_tab[i]);
        let g = 10f64.powf(gdb / 20.0);
        let thr = 10f64.powf(-gdb / 10.0);
        SatBCoeffs { g, thr, yt: g * thr, a, inv_tau: if a > 0.0 { g / a } else { 0.0 }, identity: gdb < 1e-6 }
    }
}

impl Default for SatB {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy)]
pub struct SatBCoeffs {
    pub g: f64,
    thr: f64,
    yt: f64,
    a: f64,
    inv_tau: f64,
    identity: bool,
}

impl SatBCoeffs {
    #[inline]
    pub fn eval(&self, u: f64) -> f64 {
        if self.identity {
            return u;
        }
        let a = u.abs();
        let y = if a <= self.thr { self.g * a } else { self.yt + self.a * (1.0 - (-(a - self.thr) * self.inv_tau).exp()) };
        y.copysign(u)
    }
}

/// Curve A (our "modern"): gain g below u = 1/2, C1 quadratic into (1, 1),
/// slope 4 - 3g at the ceiling, linear continuation above. Concave for
/// g <= 4/3 (+2.5 dB), which is the curve's natural maximum.
#[derive(Clone, Copy)]
pub struct SatACoeffs {
    pub g: f64,
    k: f64,
    s1: f64,
}

pub const SAT_A_MAX_G: f64 = 4.0 / 3.0;

impl SatACoeffs {
    pub fn new(g: f64) -> Self {
        let g = g.clamp(1.0, SAT_A_MAX_G);
        SatACoeffs { g, k: 4.0 * (g - 1.0), s1: (4.0 - 3.0 * g).max(0.0) }
    }
    #[inline]
    pub fn eval(&self, u: f64) -> f64 {
        let a = u.abs();
        let y = if a <= 0.5 {
            self.g * a
        } else if a <= 1.0 {
            let d = a - 0.5;
            self.g * a - self.k * d * d
        } else {
            1.0 + self.s1 * (a - 1.0)
        };
        y.copysign(u)
    }
}

/// Saturation knob (0..6, nominal dB) -> curve-A small-signal gain in dB.
/// Piecewise-linear map measured on the reference: 0 -> 0, 2 -> 1.32, 6 -> 2.50.
pub fn knob_to_gain_a_db(k: f64) -> f64 {
    let k = k.clamp(0.0, 6.0);
    let max_db = 20.0 * SAT_A_MAX_G.log10();
    if k <= 2.0 { 0.66 * k } else { 1.32 + (k - 2.0) * (max_db - 1.32) / 4.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_adaa_matches_identity_average() {
        let c = TabCurve::new(2.0, 4096, |u| u);
        let y = c.adaa(0.1, 0.3);
        assert!((y - 0.2).abs() < 1e-12);
    }

    #[test]
    fn tab_hard_clip_adaa() {
        let c = TabCurve::new(2.0, 4096, hard);
        // fully saturated segment
        assert!((c.adaa(1.5, 1.7) - HARD_LEVEL).abs() < 1e-9);
        // far outside the table
        assert!((c.adaa(50.0, 60.0) - HARD_LEVEL).abs() < 1e-9);
        // crossing the corner from 0.9 to 1.1: mean of f over the segment
        let exact = ((HARD_LEVEL * HARD_LEVEL - 0.81) / 2.0 + HARD_LEVEL * (1.1 - HARD_LEVEL)) / 0.2;
        assert!((c.adaa(0.9, 1.1) - exact).abs() < 1e-6);
    }

    #[test]
    fn sat_b_matches_manual_points() {
        let s = SatB::new();
        let c = s.coeffs(6.0);
        // +6 dB setting: linear gain of 2 below -12 dBFS, -6 dBFS out at threshold
        assert!((c.eval(0.1) - 0.1 * 10f64.powf(0.3)).abs() < 1e-12);
        let y = c.eval(10f64.powf(-12.0 / 20.0));
        assert!((20.0 * y.log10() + 6.0).abs() < 1e-9);
        // closure (b): full scale maps to full scale
        assert!((c.eval(1.0) - 1.0).abs() < 1e-9);
        // C1 at the threshold
        let t = 10f64.powf(-12.0 / 20.0);
        let e = 1e-7;
        let sl = (c.eval(t + e) - c.eval(t)) / e;
        assert!((sl - 10f64.powf(0.3)).abs() < 1e-5);
    }

    #[test]
    fn sat_a_closure_and_c1() {
        for gdb in [0.5, 1.32, 2.5] {
            let c = SatACoeffs::new(10f64.powf(gdb / 20.0));
            assert!((c.eval(1.0) - 1.0).abs() < 1e-12);
            let e = 1e-7;
            let sl = (c.eval(0.5 + e) - c.eval(0.5)) / e;
            assert!((sl - c.g).abs() < 1e-5);
        }
    }
}
