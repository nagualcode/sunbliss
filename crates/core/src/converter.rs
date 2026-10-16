//! Converter-overload clipper, running at the top oversampled rate.
//!
//! Per mode:
//!   pre-emphasis "clip filter" (optional, one pole at ~34 kHz, twice in B)
//!   -> static knee/clip curve with first-order ADAA
//!   -> (B only) overload memory: a held overload makes the plateau sag
//!      toward zero (tau ~72 ms), released within ~1 ms once the input
//!      falls back under 1.33 c. This is the "bass bloom" of the classic
//!      converter model. The reference also diverges (polarity flips, output
//!      latching) at extreme HF drive; that is deliberately not reproduced.

use crate::curves::{self, ConvB, TabCurve};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipMode {
    /// Converter A ("modern" model): short knee into a 0.98 plateau.
    ConverterA,
    /// Converter B ("classic" model): asymmetric soft compression + overload memory.
    ConverterB,
    /// Ideal clip at 0.99 c.
    Hard,
}

impl ClipMode {
    pub fn from_index(i: usize) -> Self {
        match i {
            0 => ClipMode::ConverterA,
            1 => ClipMode::ConverterB,
            _ => ClipMode::Hard,
        }
    }
    fn idx(self) -> usize {
        match self {
            ClipMode::ConverterA => 0,
            ClipMode::ConverterB => 1,
            ClipMode::Hard => 2,
        }
    }
}

/// Prewarped bilinear one-pole; 32 kHz matches the reference at a 192 kHz
/// top rate (-1.29 dB at 20 kHz). A 34 kHz analog pole maps to this.
pub const CLIP_FILTER_HZ: f64 = 32_000.0;
pub const OVERLOAD_THRESHOLD: f64 = 1.33;
pub const OVERLOAD_TAU_S: f64 = 0.072;
pub const OVERLOAD_RELEASE_S: f64 = 0.001;

/// Shared, read-only curve tables.
pub struct ClipTables {
    curves: [TabCurve; 3],
}

impl ClipTables {
    pub fn new() -> Self {
        let b = ConvB::from_measured(&curves::B_MEAS_X, &curves::B_MEAS_Y);
        ClipTables {
            curves: [
                TabCurve::new(2.0, 8192, curves::conv_a),
                TabCurve::new(2.0, 8192, move |u| b.eval(u)),
                TabCurve::new(2.0, 8192, curves::hard),
            ],
        }
    }
    pub fn curve(&self, m: ClipMode) -> &TabCurve {
        &self.curves[m.idx()]
    }
}

impl Default for ClipTables {
    fn default() -> Self {
        Self::new()
    }
}

/// Linear ramp toward a 0/1 target over a fixed number of samples.
#[derive(Clone, Copy)]
pub struct Ramp {
    pub v: f64,
    step: f64,
}

impl Ramp {
    pub fn new(v: f64) -> Self {
        Ramp { v, step: 0.0 }
    }
    pub fn set_len(&mut self, samples: f64) {
        self.step = 1.0 / samples.max(1.0);
    }
    #[inline]
    pub fn tick(&mut self, target: f64) -> f64 {
        if self.v < target {
            self.v = (self.v + self.step).min(target);
        } else if self.v > target {
            self.v = (self.v - self.step).max(target);
        }
        self.v
    }
    pub fn snap(&mut self, target: f64) {
        self.v = target;
    }
}

/// Per-block settings for the clipper.
#[derive(Clone, Copy)]
pub struct ClipSettings {
    pub enabled: bool,
    pub mode: ClipMode,
    pub filter: bool,
}

/// Per-channel clipper state.
pub struct ClipStage {
    // one-pole bilinear LPF coefficients
    b0: f64,
    a1: f64,
    lp1: (f64, f64),
    lp2: (f64, f64),
    prev_u: [f64; 2], // normalized input history: [single-filtered path, double-filtered path]
    prev_x: f64,
    mem: f64,
    mem_dec: f64,
    mem_rel: f64,
    on: Ramp,
    filt: Ramp,
    // mode crossfade: weights for A, B, Hard
    w: [Ramp; 3],
}

impl ClipStage {
    pub fn new(rate: f64) -> Self {
        let fc = CLIP_FILTER_HZ.min(0.45 * rate);
        let k = (std::f64::consts::PI * fc / rate).tan();
        let b0 = k / (1.0 + k);
        let a1 = (k - 1.0) / (k + 1.0);
        let fade = 0.010 * rate;
        let mut s = ClipStage {
            b0,
            a1,
            lp1: (0.0, 0.0),
            lp2: (0.0, 0.0),
            prev_u: [0.0; 2],
            prev_x: 0.0,
            mem: 0.0,
            mem_dec: 1.0 - (-1.0 / (OVERLOAD_TAU_S * rate)).exp(),
            mem_rel: 1.0 - (-1.0 / (OVERLOAD_RELEASE_S * rate)).exp(),
            on: Ramp::new(1.0),
            filt: Ramp::new(0.0),
            w: [Ramp::new(0.0), Ramp::new(1.0), Ramp::new(0.0)],
        };
        s.on.set_len(fade);
        s.filt.set_len(fade);
        s.w.iter_mut().for_each(|r| r.set_len(fade));
        s
    }

    pub fn reset(&mut self) {
        self.lp1 = (0.0, 0.0);
        self.lp2 = (0.0, 0.0);
        self.prev_u = [0.0; 2];
        self.prev_x = 0.0;
        self.mem = 0.0;
    }

    /// Jump all crossfades to their targets (used on prepare / reset).
    pub fn snap(&mut self, s: &ClipSettings) {
        self.on.snap(if s.enabled { 1.0 } else { 0.0 });
        self.filt.snap(if s.filter { 1.0 } else { 0.0 });
        for (i, r) in self.w.iter_mut().enumerate() {
            r.snap(if i == s.mode.idx() { 1.0 } else { 0.0 });
        }
    }

    #[inline]
    fn lp(&self, st: &mut (f64, f64), x: f64) -> f64 {
        // y = b0 x + b0 x1 - a1 y1
        let y = self.b0 * (x + st.0) - self.a1 * st.1;
        st.0 = x;
        st.1 = if y.abs() < 1e-30 { 0.0 } else { y };
        st.1
    }

    /// Process `top` in place. `ceil` holds the (smoothed, linear) ceiling for
    /// each base-rate sample; `l` is the oversampling factor. When `adaa` is
    /// false (no oversampling) the curves are applied directly.
    pub fn process(&mut self, tables: &ClipTables, top: &mut [f64], ceil: &[f64], l: usize, s: &ClipSettings, adaa: bool) {
        let t_on = if s.enabled { 1.0 } else { 0.0 };
        let t_f = if s.filter { 1.0 } else { 0.0 };
        let mi = s.mode.idx();
        for (j, x) in top.iter_mut().enumerate() {
            let c = ceil[j / l];
            let inv_c = 1.0 / c;
            let xin = *x;
            let f1 = { let mut st = self.lp1; let y = self.lp(&mut st, xin); self.lp1 = st; y };
            let f2 = { let mut st = self.lp2; let y = self.lp(&mut st, f1); self.lp2 = st; y };
            let fw = self.filt.tick(t_f);
            let on = self.on.tick(t_on);
            let xs = [xin + fw * (f1 - xin), xin + fw * (f2 - xin)];
            let us = [xs[0] * inv_c, xs[1] * inv_c];

            // identity path (clip off): ADAA of identity == two-sample average
            let ident = if adaa { 0.5 * (xin + self.prev_x) } else { xin };
            self.prev_x = xin;

            let mut acc = 0.0;
            for m in 0..3 {
                let w = self.w[m].tick(if m == mi { 1.0 } else { 0.0 });
                if w == 0.0 {
                    continue;
                }
                let path = if m == 1 { 1 } else { 0 };
                let curve = &tables.curves[m];
                let mut y = if adaa { curve.adaa(self.prev_u[path], us[path]) } else { curve.f(us[path]) };
                if m == 1 {
                    let over = us[1].abs() > OVERLOAD_THRESHOLD;
                    self.mem += if over { (1.0 - self.mem) * self.mem_dec } else { -self.mem * self.mem_rel };
                    if self.mem < 1e-30 {
                        self.mem = 0.0;
                    }
                    y *= 1.0 - self.mem;
                }
                acc += w * y;
            }
            self.prev_u = us;
            let clipped = acc * c;
            *x = ident + on * (clipped - ident);
        }
    }
}
