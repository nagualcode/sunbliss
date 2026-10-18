//! Contour (the reference's "Alchemy", "MX" inside its binary): high-band
//! softening near the ceiling, applied sample by sample.
//!
//! Band split (top rate): linear-phase low-pass with the reference's
//! magnitude, 1 / (1 + (f/fc)^4) (a zero-phase Linkwitz-Riley / squared
//! 2nd-order Butterworth, fc 2965 Hz, fitted to its frequency shape within
//! 0.0024); high band = delayed input - low band, so the split sums back
//! exactly. out = low + k * high.
//!
//! The low-pass is an interpolated FIR: a short FIR designed at the base
//! rate and run with its taps M = top/base samples apart, times two
//! length-M boxcars that null its images at multiples of the base rate.
//! About 1.6 ms of taps per base sample instead of per top sample; the
//! history is kept per phase (n mod M) so each output is one contiguous
//! dot product (the taps are symmetric, so no reversal is needed).
//!
//! k follows the wide-band level |x|/c of the (already clipped) signal
//! almost instantly, like the reference: a loud low end modulates the high
//! band with its own waveform (AM sidebands), as a level-dependent analog
//! stage would. The static law k(level, amount) is the reference's,
//! measured on an 8 kHz probe riding DC plateaus (lab/probe_alchemy_inst.py,
//! docs/data/gc_alchemy_inst.json). The law output is then smoothed as a
//! cubic power mean, k = LPF(k_inst^3)^(1/3), one pole, tau 0.4 ms: below
//! ~100 Hz the gain tracks the waveform, above ~500 Hz the ripple is gone
//! (the reference's AM sidebands fall 6 dB/oct from there) and the mean
//! settles on the power mean, 0.3 dB less reduction than the plain mean, as
//! measured on carriers from 1.5 Hz to 1 kHz (lab/probe_alchemy_mod.py).

use crate::design::freq_sampled;
use crate::fir::{dot, Delay};

/// Crossover of the band split (Hz).
pub const SPLIT_HZ: f64 = 2965.0;
/// Span of the base-rate FIR (s).
const SPAN_S: f64 = 0.0016;
/// Gain smoothing time constant (s).
pub const SMOOTH_TAU_S: f64 = 0.0004;

struct Boxcar {
    buf: Vec<f64>,
    pos: usize,
    sum: f64,
    inv: f64,
}

impl Boxcar {
    fn new(m: usize) -> Self {
        Boxcar { buf: vec![0.0; m], pos: 0, sum: 0.0, inv: 1.0 / m as f64 }
    }
    fn reset(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 0.0);
        self.sum = 0.0;
        self.pos = 0;
    }
    #[inline]
    fn tick(&mut self, x: f64) -> f64 {
        self.sum += x - self.buf[self.pos];
        self.buf[self.pos] = x;
        self.pos += 1;
        if self.pos == self.buf.len() {
            self.pos = 0;
            // exact re-sum once per period: no drift, O(1) amortised
            self.sum = self.buf.iter().sum();
        }
        self.sum * self.inv
    }
}

/// Measured law grid, band gain k in dB: amounts 0, 0.25 .. 2 dB; levels
/// -14 .. 0 dB re ceiling in 0.5 dB steps (the 0 dB column is extrapolated
/// from the last two). k is solved from the 8 kHz probe gain g as
/// (g - LP(8k)) / HP(8k), since the split still passes 1.9 % low band there.
pub const LAW_NA: usize = 9;
pub const LAW_NL: usize = 29;
pub const LAW_DA: f64 = 0.25;
pub const LAW_L0_DB: f64 = -14.0;
pub const LAW_DL_DB: f64 = 0.5;
#[rustfmt::skip]
pub const LAW_DB: [[f64; LAW_NL]; LAW_NA] = [
    [0.0; LAW_NL],
    [0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, -0.2894, -0.5718, -0.8496, -1.1274],
    [0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, -0.0929, -0.3824, -0.6650, -0.9408, -1.2089, -1.4678, -1.7217, -1.9756],
    [0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, -0.0521, -0.3399, -0.6227, -0.8988, -1.1681, -1.4302, -1.6855, -1.9331, -2.1716, -2.4051, -2.6386],
    [0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, -0.1728, -0.4594, -0.7376, -1.0137, -1.2793, -1.5380, -1.7899, -2.0346, -2.2723, -2.5024, -2.7238, -2.9401, -3.1563],
    [0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, -0.1841, -0.4701, -0.7513, -1.0235, -1.2873, -1.5485, -1.7996, -2.0435, -2.2807, -2.5106, -2.7338, -2.9495, -3.1565, -3.3584, -3.5603],
    [0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, -0.0951, -0.3837, -0.6673, -0.9414, -1.2103, -1.4704, -1.7220, -1.9710, -2.2098, -2.4416, -2.6666, -2.8844, -3.0955, -3.2993, -3.4947, -3.6849, -3.8751],
    [0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, -0.2092, -0.4963, -0.7750, -1.0486, -1.3128, -1.5716, -1.8218, -2.0633, -2.3022, -2.5310, -2.7529, -2.9681, -3.1760, -3.3774, -3.5715, -3.7575, -3.9384, -4.1193],
    [0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, -0.2467, -0.5306, -0.8097, -1.0804, -1.3460, -1.6021, -1.8529, -2.0949, -2.3284, -2.5591, -2.7801, -2.9939, -3.2012, -3.4014, -3.5949, -3.7814, -3.9599, -4.1335, -4.3070],
];

/// Points per law row on the linear level axis u = |x|/c in [0, 1].
const NU: usize = 1024;

/// The law resampled on a linear level axis (no log per sample), one row per
/// grid amount, as linear gains.
pub struct Law {
    rows: Vec<[f64; NU + 2]>,
}

impl Law {
    pub fn new() -> Self {
        let rows = (0..LAW_NA)
            .map(|ai| {
                let mut r = [1.0; NU + 2];
                for (j, v) in r.iter_mut().enumerate().take(NU + 1) {
                    *v = 10f64.powf(Self::grid_db(ai, j as f64 / NU as f64) / 20.0);
                }
                r[NU + 1] = r[NU];
                r
            })
            .collect();
        Law { rows }
    }

    fn grid_db(ai: usize, u: f64) -> f64 {
        if u <= 0.0 {
            return 0.0;
        }
        let p = (20.0 * u.log10() - LAW_L0_DB) / LAW_DL_DB;
        if p <= 0.0 {
            return LAW_DB[ai][0];
        }
        let i = (p as usize).min(LAW_NL - 2);
        let t = (p - i as f64).min(1.0);
        LAW_DB[ai][i] + t * (LAW_DB[ai][i + 1] - LAW_DB[ai][i])
    }

    /// Linear gain for level `u` (|x|/c, clamped to 1) and amount (dB, 0..2).
    #[inline]
    pub fn k(&self, u: f64, amount: f64) -> f64 {
        if amount <= 0.0 {
            return 1.0;
        }
        let pa = (amount / LAW_DA).min((LAW_NA - 1) as f64);
        let ai = (pa as usize).min(LAW_NA - 2);
        let ta = pa - ai as f64;
        let pu = u.min(1.0) * NU as f64;
        let ui = pu as usize;
        let tu = pu - ui as f64;
        let (r0, r1) = (&self.rows[ai], &self.rows[ai + 1]);
        let k0 = r0[ui] + tu * (r0[ui + 1] - r0[ui]);
        let k1 = r1[ui] + tu * (r1[ui + 1] - r1[ui]);
        k0 + ta * (k1 - k0)
    }
}

impl Default for Law {
    fn default() -> Self {
        Self::new()
    }
}

/// Top-rate band split and gain application, per channel.
pub struct ContourStage {
    m: usize,
    /// base-rate FIR taps (applied M top samples apart), symmetric
    g: Vec<f64>,
    /// per-phase histories, M rings of 2n (written twice so a window is
    /// contiguous), and the write position shared by all phases
    hist: Vec<f64>,
    phase: usize,
    hpos: usize,
    boxes: [Boxcar; 2],
    delay: Delay,
    /// smoothed k^3, its one-pole coefficient, and the running cube root
    k3: f64,
    alpha: f64,
    kc: f64,
}

impl ContourStage {
    pub fn new(base_rate: f64, top_rate: f64) -> Self {
        let m = ((top_rate / base_rate).round() as usize).max(1);
        let mut n = (SPAN_S * base_rate).round() as usize;
        if n % 2 == 0 {
            n += 1;
        }
        let fc = SPLIT_HZ / base_rate;
        // the boxcar pair's droop is divided out of the FIR target
        let boxes2 = |f: f64| {
            if m == 1 || f == 0.0 {
                1.0
            } else {
                let b = (std::f64::consts::PI * f).sin() / (m as f64 * (std::f64::consts::PI * f / m as f64).sin());
                b * b
            }
        };
        // rectangular window: the response has decayed to ~1e-5 at the span edges,
        // and any taper would bend the narrow main lobe
        let g = freq_sampled(n, 0.0, |f| 1.0 / (1.0 + (f / fc).powi(4)) / boxes2(f));
        let mut delay = Delay::new((n - 1) * m / 2 + m);
        delay.set((n - 1) * m / 2 + (m - 1));
        ContourStage {
            m,
            g,
            hist: vec![0.0; 2 * n * m],
            phase: 0,
            hpos: 0,
            boxes: [Boxcar::new(m), Boxcar::new(m)],
            delay,
            k3: 1.0,
            kc: 1.0,
            alpha: 1.0 - (-1.0 / (SMOOTH_TAU_S * top_rate)).exp(),
        }
    }

    /// Group delay in top-rate samples.
    pub fn delay(&self) -> usize {
        self.delay.delay()
    }

    pub fn reset(&mut self) {
        self.hist.iter_mut().for_each(|v| *v = 0.0);
        self.phase = 0;
        self.hpos = 0;
        self.boxes.iter_mut().for_each(|b| b.reset());
        self.delay.reset();
        self.k3 = 1.0;
        self.kc = 1.0;
    }

    /// Store `x`; returns the start of its phase's window (oldest..newest).
    #[inline]
    fn push(&mut self, x: f64) -> usize {
        let n = self.g.len();
        let base = self.phase * 2 * n;
        let r = self.hpos;
        self.hist[base + r] = x;
        self.hist[base + r + n] = x;
        self.phase += 1;
        if self.phase == self.m {
            self.phase = 0;
            self.hpos = if r + 1 == n { 0 } else { r + 1 };
        }
        base + r + 1
    }

    #[inline]
    fn low(&mut self, x: f64) -> f64 {
        let w0 = self.push(x);
        let acc = dot(&self.g, &self.hist[w0..w0 + self.g.len()]);
        let y = self.boxes[0].tick(acc);
        self.boxes[1].tick(y)
    }

    /// Process `top` in place. `amount` (dB) and `ceil` (linear) hold one
    /// value per base sample; `l` is the oversampling factor.
    pub fn process(&mut self, law: &Law, top: &mut [f64], amount: &[f64], ceil: &[f64], l: usize) {
        for (j, x) in top.iter_mut().enumerate() {
            let i = j / l;
            let a = amount[i];
            if a <= 0.0 && self.k3 == 1.0 {
                self.kc = 1.0;
                // off and settled: keep the history and the delay running
                // (no stale state when it comes back), skip the filter
                self.push(*x);
                *x = self.delay.tick(*x);
                continue;
            }
            let lp = self.low(*x);
            let xd = self.delay.tick(*x);
            let k = law.k(xd.abs() / ceil[i], a);
            self.k3 += (k * k * k - self.k3) * self.alpha;
            if 1.0 - self.k3 < 1e-12 {
                self.k3 = 1.0;
            }
            // one Newton step from the previous root (k3 moves little per
            // sample, so this converges quadratically; exact cbrt per block)
            let c = self.kc;
            self.kc = c - (c * c * c - self.k3) / (3.0 * c * c);
            *x = lp + self.kc * (xd - lp);
        }
        self.kc = self.k3.cbrt();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_sums_back_when_off() {
        let law = Law::new();
        let mut c = ContourStage::new(48_000.0, 192_000.0);
        let d = c.delay();
        let x: Vec<f64> = (0..4000).map(|i| ((i * 7919) % 1000) as f64 / 1000.0 - 0.5).collect();
        let mut y = x.clone();
        c.process(&law, &mut y, &[0.0; 1000], &[1.0; 1000], 4);
        for i in d..4000 {
            assert_eq!(y[i], x[i - d]);
        }
    }

    #[test]
    fn split_magnitude_is_linkwitz_riley() {
        let mut c = ContourStage::new(48_000.0, 192_000.0);
        let d = c.delay();
        let n = 1 << 14;
        let mut h = vec![0.0; n];
        for i in 0..n {
            h[i] = c.low(if i == 0 { 1.0 } else { 0.0 });
        }
        assert!(h[..d].iter().zip(h[d + 1..2 * d + 1].iter().rev()).all(|(a, b)| (a - b).abs() < 1e-12));
        for f in [100.0, 1000.0, 2000.0, 2965.0, 4000.0, 8000.0, 16000.0] {
            let w = 2.0 * std::f64::consts::PI * f / 192_000.0;
            let re: f64 = h.iter().enumerate().map(|(i, v)| v * (w * (i as f64 - d as f64)).cos()).sum();
            let want = 1.0 / (1.0 + (f / SPLIT_HZ).powi(4));
            assert!((re - want).abs() < 2e-4, "{f}: {re} vs {want}");
        }
    }

    #[test]
    fn law_matches_grid() {
        let law = Law::new();
        assert_eq!(law.k(0.1, 2.0), 1.0);
        // grid point: amount 2, -2 dB re ceiling
        let u = 10f64.powf(-2.0 / 20.0);
        let want = LAW_DB[8][24];
        assert!((20.0 * law.k(u, 2.0).log10() - want).abs() < 0.01);
        // monotone in level and amount
        assert!(law.k(0.9, 2.0) < law.k(0.8, 2.0));
        assert!(law.k(0.9, 2.0) < law.k(0.9, 1.0));
    }
}
