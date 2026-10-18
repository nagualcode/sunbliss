//! Static transfer curves.
//!
//! Clip curves are tabulated together with their first and second
//! antiderivatives so the clipper can run second-order antiderivative
//! anti-aliasing (ADAA2) for any shape. Saturation curves are closed-form (they are C1 and only act below
//! the ceiling, so they alias far less than the clip corner).

/// A memoryless curve f(u), tabulated on [-R, R] with its antiderivatives
/// F (= F1) and F2. Outside the table f is held constant (all clip curves
/// saturate before R), so F1 continues linearly and F2 quadratically.
pub struct TabCurve {
    r: f64,
    inv_du: f64,
    du: f64,
    f: Vec<f64>,
    big_f: Vec<f64>,
    big_f2: Vec<f64>,
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
        // F2 by integrating the cubic Hermite of F exactly on each cell:
        // int_0^1 H(t) dt = (p0 + p1)/2 + (m0 - m1)/12   (in cell units)
        let mut big_f2 = vec![0.0; n + 1];
        for i in 0..n {
            let (p0, p1) = (big_f[i], big_f[i + 1]);
            let (m0, m1) = (f[i] * du, f[i + 1] * du);
            big_f2[i + 1] = big_f2[i] + du * (0.5 * (p0 + p1) + (m0 - m1) / 12.0);
        }
        let mid2 = big_f2[n / 2];
        big_f2.iter_mut().for_each(|v| *v -= mid2);
        TabCurve { r, inv_du: 1.0 / du, du, f, big_f, big_f2 }
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

    /// F2(u): quintic Hermite on (F2, F, f) nodes; quadratic continuation outside.
    #[inline]
    pub fn big_f2(&self, u: f64) -> f64 {
        let n = self.f.len() - 1;
        if u >= self.r {
            let d = u - self.r;
            return self.big_f2[n] + self.big_f[n] * d + 0.5 * self.f[n] * d * d;
        }
        if u <= -self.r {
            let d = u + self.r;
            return self.big_f2[0] + self.big_f[0] * d + 0.5 * self.f[0] * d * d;
        }
        let (i, t) = self.cell(u);
        let h = self.du;
        let (p0, p1) = (self.big_f2[i], self.big_f2[i + 1]);
        let (v0, v1) = (self.big_f[i] * h, self.big_f[i + 1] * h);
        let (a0, a1) = (self.f[i] * h * h, self.f[i + 1] * h * h);
        let t2 = t * t;
        let t3 = t2 * t;
        let t4 = t3 * t;
        let t5 = t4 * t;
        let h0 = 1.0 - 10.0 * t3 + 15.0 * t4 - 6.0 * t5;
        let h1 = t - 6.0 * t3 + 8.0 * t4 - 3.0 * t5;
        let h2 = 0.5 * t2 - 1.5 * t3 + 1.5 * t4 - 0.5 * t5;
        let h3 = 0.5 * t3 - t4 + 0.5 * t5;
        let h4 = -4.0 * t3 + 7.0 * t4 - 3.0 * t5;
        let h5 = 10.0 * t3 - 15.0 * t4 + 6.0 * t5;
        h0 * p0 + h1 * v0 + h2 * a0 + h3 * a1 + h4 * v1 + h5 * p1
    }

    /// Divided difference of F2, falling back to F1 at the midpoint.
    #[inline]
    pub fn d2(&self, a: f64, b: f64) -> f64 {
        let d = a - b;
        if d.abs() > 1e-6 { (self.big_f2(a) - self.big_f2(b)) / d } else { self.big_f(0.5 * (a + b)) }
    }

    /// Divided difference of F2 with F2(b) already known.
    #[inline]
    pub fn d2_known(&self, a: f64, fa: f64, b: f64, fb: f64) -> f64 {
        let d = a - b;
        if d.abs() > 1e-6 { (fa - fb) / d } else { self.big_f(0.5 * (a + b)) }
    }

    /// ADAA2 from a fresh divided difference `d01` = D(u0,u1) and the previous
    /// one `d12` = D(u1,u2) (callers cache it: one F2 evaluation per sample).
    #[inline]
    pub fn adaa2_cached(&self, u0: f64, u1: f64, u2: f64, d01: f64, d12: f64) -> f64 {
        let d02 = u0 - u2;
        if d02.abs() > 1e-5 {
            return 2.0 / d02 * (d01 - d12);
        }
        self.adaa2(u0, u1, u2)
    }

    /// Second-order ADAA over u2 -> u1 -> u0 (u0 newest). Integer delay of one
    /// sample; in the linear region it equals the 3-tap mean (u0+u1+u2)/3.
    #[inline]
    pub fn adaa2(&self, u0: f64, u1: f64, u2: f64) -> f64 {
        let d02 = u0 - u2;
        if d02.abs() > 1e-5 {
            return 2.0 / d02 * (self.d2(u0, u1) - self.d2(u1, u2));
        }
        // u0 ~ u2: expand around their mean (Bilbao et al. ill-conditioning fix)
        let xb = 0.5 * (u0 + u2);
        let delta = xb - u1;
        if delta.abs() > 1e-5 {
            2.0 / delta * (self.big_f(xb) + (self.big_f2(u1) - self.big_f2(xb)) / delta)
        } else {
            self.f(0.5 * (xb + u1))
        }
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

/// Converter B ("classic"): a degree-9 polynomial, least-squares fitted to the
/// reference's DC transfer on |u| <= 1.31 (lab/fit_classic.py,
/// docs/data/fit_classic_poly.json; residual 1.4e-7, the f32 floor of the
/// host I/O). Odd terms give the gentle compression into a ~0.97 plateau,
/// the small even terms the 2nd-harmonic asymmetry. Coefficients in
/// increasing powers of u.
pub const CLASSIC_POLY: [f64; 10] = [
    0.0,
    9.9938217277e-01,
    -2.3697332645e-02,
    -2.5738858649e-01,
    3.6481392458e-02,
    8.6339251973e-01,
    -8.7098505314e-03,
    -9.3910006576e-01,
    -2.5033402721e-03,
    2.7521139794e-01,
];

/// Edge of the polynomial's valid range. Past it the polynomial diverges;
/// the curve holds its edge value and the overload memory takes over
/// (converter.rs).
pub const CLASSIC_RANGE: f64 = 1.32;

pub fn conv_b(u: f64) -> f64 {
    let u = u.clamp(-CLASSIC_RANGE, CLASSIC_RANGE);
    CLASSIC_POLY.iter().rev().fold(0.0, |acc, &c| acc * u + c)
}

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
    fn adaa2_linear_region_is_three_tap_mean() {
        let c = TabCurve::new(2.0, 8192, |u| u);
        let y = c.adaa2(0.3, 0.1, -0.2);
        assert!((y - (0.3 + 0.1 - 0.2) / 3.0).abs() < 1e-9, "{y}");
        let y = c.adaa2(0.2, 0.1, 0.2);
        assert!((y - 0.5 / 3.0).abs() < 1e-6, "{y}");
    }

    #[test]
    fn adaa2_saturated_region() {
        let c = TabCurve::new(2.0, 8192, hard);
        assert!((c.adaa2(1.5, 1.7, 1.6) - HARD_LEVEL).abs() < 1e-7);
        assert!((c.adaa2(40.0, 60.0, 50.0) - HARD_LEVEL).abs() < 1e-7);
        assert!((c.adaa2(-40.0, -60.0, -50.0) + HARD_LEVEL).abs() < 1e-7);
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
