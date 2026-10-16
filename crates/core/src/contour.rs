//! Contour: dynamic high-band softening near the ceiling.
//!
//! Band split (top rate): linear-phase low-pass made of four cascaded
//! boxcars (Gaussian-like, -6 dB near 3.1 kHz, O(1) per sample),
//! high band = delayed input - low band, so the split sums back exactly.
//! out = low + k * high, with k <= 1 from the detector.
//!
//! Detector (base rate): |x|/c, clamped to 1, centred sliding max followed by
//! a centred moving average, using lookahead instead of attack/release. The
//! reference modulates the high band sample by sample, which writes the
//! waveform of the loud low end onto the cymbals as AM sidebands; following
//! the peak envelope instead removes most of that.

use crate::fir::Delay;

pub const BOXCARS: usize = 4;
/// Target fs/M for the boxcars (sets the split around 3 kHz).
pub const BOX_RATE_HZ: f64 = 9_600.0;
/// Total detector lookahead (s): half for the max window, half for smoothing.
pub const LOOKAHEAD_S: f64 = 0.005;
/// Level (dB re ceiling) where the softening starts.
pub const ONSET_DB: f64 = -9.0;
pub const SHAPE_EXP: f64 = 1.25;

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

/// Top-rate band split and gain application, per channel.
pub struct ContourStage {
    boxes: Vec<Boxcar>,
    delay: Delay,
    k_prev: f64,
}

impl ContourStage {
    pub fn new(top_rate: f64) -> Self {
        let m = ((top_rate / BOX_RATE_HZ).round() as usize).max(2);
        let mut delay = Delay::new(BOXCARS * m);
        delay.set(Self::delay_for(m));
        ContourStage { boxes: (0..BOXCARS).map(|_| Boxcar::new(m)).collect(), delay, k_prev: 1.0 }
    }

    fn delay_for(m: usize) -> usize {
        BOXCARS * (m - 1) / 2
    }

    /// Group delay in top-rate samples.
    pub fn delay(&self) -> usize {
        self.delay.delay()
    }

    pub fn reset(&mut self) {
        self.boxes.iter_mut().for_each(|b| b.reset());
        self.delay.reset();
        self.k_prev = 1.0;
    }

    /// `k` holds one gain per base sample (already aligned); interpolated
    /// linearly across the `l` top samples of each base period.
    pub fn process(&mut self, top: &mut [f64], k: &[f64], l: usize) {
        let inv_l = 1.0 / l as f64;
        for (j, x) in top.iter_mut().enumerate() {
            let mut lp = *x;
            for b in self.boxes.iter_mut() {
                lp = b.tick(lp);
            }
            let xd = self.delay.tick(*x);
            let i = j / l;
            let t = ((j % l) + 1) as f64 * inv_l;
            let k0 = if i == 0 { self.k_prev } else { k[i - 1] };
            let kk = k0 + t * (k[i] - k0);
            *x = if kk >= 1.0 { xd } else { lp + kk * (xd - lp) };
        }
        if let Some(&last) = k.last() {
            self.k_prev = last;
        }
    }
}

/// Base-rate envelope detector with centred max + centred average.
pub struct Detector {
    h1: usize,
    h2: usize,
    // sliding max over 2*h1+1 (ring of values, recomputed lazily)
    win: Vec<f64>,
    wpos: usize,
    cur_max: f64,
    max_age: usize,
    // moving average over 2*h2+1
    avg: Vec<f64>,
    apos: usize,
    asum: f64,
}

impl Detector {
    pub fn new(fs: f64) -> Self {
        let h = ((LOOKAHEAD_S * fs) as usize).max(2);
        let h1 = h / 2;
        let h2 = h - h1;
        Detector {
            h1,
            h2,
            win: vec![0.0; 2 * h1 + 1],
            wpos: 0,
            cur_max: 0.0,
            max_age: 0,
            avg: vec![0.0; 2 * h2 + 1],
            apos: 0,
            asum: 0.0,
        }
    }

    /// Lookahead in base samples (the signal must be delayed by this much).
    pub fn lookahead(&self) -> usize {
        self.h1 + self.h2
    }

    pub fn reset(&mut self) {
        self.win.iter_mut().for_each(|v| *v = 0.0);
        self.avg.iter_mut().for_each(|v| *v = 0.0);
        self.asum = 0.0;
        self.cur_max = 0.0;
        self.max_age = 0;
        self.wpos = 0;
        self.apos = 0;
    }

    /// Push one normalised level (|x|/c clamped to 1); returns the smoothed
    /// envelope centred `lookahead()` samples in the past.
    #[inline]
    pub fn tick(&mut self, e: f64) -> f64 {
        let n = self.win.len();
        self.win[self.wpos] = e;
        self.wpos = (self.wpos + 1) % n;
        self.max_age += 1;
        if e >= self.cur_max {
            self.cur_max = e;
            self.max_age = 0;
        } else if self.max_age >= n {
            // the max left the window: rescan (amortised cheap: windows are short)
            let mut m = 0.0;
            let mut age = 0;
            for k in 0..n {
                let idx = (self.wpos + n - 1 - k) % n;
                if self.win[idx] > m {
                    m = self.win[idx];
                    age = k;
                }
            }
            self.cur_max = m;
            self.max_age = age;
        }
        let na = self.avg.len();
        self.asum += self.cur_max - self.avg[self.apos];
        self.avg[self.apos] = self.cur_max;
        self.apos = (self.apos + 1) % na;
        if self.apos == 0 {
            self.asum = self.avg.iter().sum();
        }
        (self.asum / na as f64).max(0.0)
    }
}

/// Gain law: k (linear) for a smoothed envelope `env` (0..1 re ceiling) and
/// an amount in dB (0..2).
#[inline]
pub fn law(env: f64, amount_db: f64) -> f64 {
    if amount_db <= 0.0 || env <= 0.0 {
        return 1.0;
    }
    let env_db = 20.0 * env.log10();
    let u = ((env_db - ONSET_DB) / -ONSET_DB).clamp(0.0, 1.0);
    if u == 0.0 {
        return 1.0;
    }
    let gr_db = -amount_db * u.powf(SHAPE_EXP);
    10f64.powf(gr_db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_sums_back_when_k_is_one() {
        let mut c = ContourStage::new(192_000.0);
        let d = c.delay();
        let x: Vec<f64> = (0..4000).map(|i| ((i * 7919) % 1000) as f64 / 1000.0 - 0.5).collect();
        let mut y = x.clone();
        let k = vec![1.0; 1000];
        c.process(&mut y, &k, 4);
        for i in d..4000 {
            assert_eq!(y[i], x[i - d]);
        }
    }

    #[test]
    fn detector_is_centred() {
        let mut det = Detector::new(48_000.0);
        let la = det.lookahead();
        let mut out = Vec::new();
        for i in 0..2000 {
            out.push(det.tick(if i == 1000 { 1.0 } else { 0.0 }));
        }
        // response to an impulse is symmetric around 1000 + la
        let c = 1000 + la;
        for k in 1..la {
            assert!((out[c - k] - out[c + k]).abs() < 1e-12, "{k}");
        }
    }

    #[test]
    fn law_matches_calibration() {
        assert_eq!(law(10f64.powf(-12.0 / 20.0), 2.0), 1.0);
        let k = law(1.0, 2.0);
        assert!((20.0 * k.log10() + 2.0).abs() < 1e-12);
    }
}
