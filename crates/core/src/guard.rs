//! Overshoot Guard: keeps the *true* (inter-sample) peak under the ceiling.
//!
//! The reference only clamps sample peaks, so after its downsampler the true
//! peak can sit up to ~1 dB above the ceiling. Here:
//!   1. 4x polyphase interpolation + parabolic refinement estimates
//!      inter-sample peaks;
//!   2. required gain r = min(1, c / peak) per sample, linked across channels;
//!   3. sliding minimum over K+1+2H samples followed by a (K+1)-sample moving
//!      average: every averaged window contains the minimum of r over
//!      +-H samples around the output sample, so the whole neighbourhood of
//!      a peak gets a gain <= r at the peak (no overshoot), and the gain
//!      moves smoothly (no clicks);
//!   4. an upward-only release keeps recovery gentle.
//! Audio is delayed by K + interpolator delay; the delay is always present so
//! latency does not change when the guard is toggled.

use crate::design::{kaiser_beta, lowpass};
use crate::fir::Delay;

pub const MAX_CH: usize = 8;
const OSF: usize = 4;
const ISP_TAPS: usize = 185; // delay 92 at 4x = 23 base samples
const ISP_DELAY: usize = 23;
const PH: usize = 47; // taps per phase (ceil(185/4))
pub const WINDOW_S: f64 = 0.001;
/// Samples on each side of a peak that share its gain (sinc-tail coverage).
const HOLD: usize = 16;
pub const RELEASE_S: f64 = 0.010;
/// Safety margin under the ceiling (dB): covers the estimator error for
/// content between 0.45 fs and Nyquist, where no finite interpolator can
/// separate signal from image (measured worst case ~0.13 dB on noise clipped 12 dB).
pub const MARGIN_DB: f64 = 0.15;

/// What the limiter keeps under the ceiling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardMode {
    /// Delay only.
    Off,
    /// Sample peaks <= ceiling (replaces a hard clamp, which aliases). The
    /// detector still uses the true-peak estimate (>= every sample peak,
    /// and constant on steady tones, where sample peaks beat against fs and
    /// would modulate the gain), just without the safety margin.
    Sample,
    /// Inter-sample (true) peaks.
    TruePeak,
}

pub struct Guard {
    phases: [[f64; PH]; OSF], // reversed polyphase coefficients
    hist: Vec<[f64; 2 * PH]>, // per channel ring, written twice so a window is contiguous
    hpos: usize,
    prev_last: Vec<f64>, // last interpolated value of the previous sample (per channel)   // per channel last PH inputs (last = newest)
    audio: Vec<Delay>,
    k: usize,
    // sliding min (ring) and moving average
    rq: Vec<f64>,
    rpos: usize,
    cur_min: f64,
    min_age: usize,
    avg: Vec<f64>,
    apos: usize,
    asum: f64,
    g: f64,
    rel: f64,
    pub last_gr_db: f64,
    #[cfg(test)]
    max_est: f64,
}

impl Guard {
    pub fn new(fs: f64, channels: usize) -> Self {
        // The guard sees the decimator output, band-limited to 0.4535 fs, so
        // the interpolator must be flat to there and reject images from
        // 0.5465 fs: transition centred on Nyquist.
        let h = lowpass(ISP_TAPS, 0.5 / OSF as f64, kaiser_beta(70.0));
        let mut phases = [[0.0; PH]; OSF];
        for (p, ph) in phases.iter_mut().enumerate() {
            for j in 0..PH {
                let idx = OSF * j + p;
                ph[PH - 1 - j] = if idx < ISP_TAPS { OSF as f64 * h[idx] } else { 0.0 };
            }
        }
        let k = ((WINDOW_S * fs).round() as usize).max(8);
        let mut audio: Vec<Delay> = (0..channels).map(|_| Delay::new(k + HOLD + ISP_DELAY + 1)).collect();
        audio.iter_mut().for_each(|d| d.set(k + HOLD + ISP_DELAY));
        Guard {
            phases,
            hist: vec![[0.0; 2 * PH]; channels],
            hpos: 0,
            prev_last: vec![0.0; channels],
            audio,
            k,
            rq: vec![1.0; k + 1 + 2 * HOLD],
            rpos: 0,
            cur_min: 1.0,
            min_age: 0,
            avg: vec![1.0; k + 1],
            apos: 0,
            asum: (k + 1) as f64,
            g: 1.0,
            rel: 1.0 - (-1.0 / (RELEASE_S * fs)).exp(),
            last_gr_db: 0.0,
            #[cfg(test)]
            max_est: 0.0,
        }
    }

    pub fn delay(&self) -> usize {
        self.k + HOLD + ISP_DELAY
    }

    pub fn reset(&mut self) {
        self.hist.iter_mut().for_each(|h| *h = [0.0; 2 * PH]);
        self.hpos = 0;
        self.prev_last.iter_mut().for_each(|v| *v = 0.0);
        self.audio.iter_mut().for_each(|d| d.reset());
        self.rq.iter_mut().for_each(|v| *v = 1.0);
        self.avg.iter_mut().for_each(|v| *v = 1.0);
        self.asum = (self.k + 1) as f64;
        self.cur_min = 1.0;
        self.min_age = 0;
        self.g = 1.0;
    }

    /// Process all channels in place for `n` samples.
    /// `ceil` is the per-sample ceiling (linear).
    pub fn process(&mut self, chans: &mut [&mut [f64]], n: usize, ceil: &[f64], mode: GuardMode) {
        let on = mode != GuardMode::Off;
        let tp = mode == GuardMode::TruePeak;
        let target = if tp { 10f64.powf(-MARGIN_DB / 20.0) } else { 1.0 };
        let mut min_g: f64 = 1.0;
        for i in 0..n {
            // 1+2: inter-sample peak, linked
            let mut peak: f64 = 0.0;
            let hp = self.hpos;
            for (ch, buf) in chans.iter().enumerate() {
                let ring = &mut self.hist[ch];
                ring[hp] = buf[i];
                ring[hp + PH] = buf[i];
                if !on {
                    continue;
                }
                // oldest..newest window, newest at index PH-1
                let h = &ring[hp + 1..hp + 1 + PH];
                // v[0] = previous grid point, v[1..=OSF] = this sample's phases,
                // v[OSF+1] = the next original sample (phase 0 of the next period)
                let mut v = [0.0f64; OSF + 2];
                v[0] = self.prev_last[ch];
                for (p, ph) in self.phases.iter().enumerate() {
                    v[p + 1] = crate::fir::dot(ph, h);
                }
                v[OSF + 1] = h[PH - 1 - (ISP_DELAY - 1)];
                self.prev_last[ch] = v[OSF];
                // parabolic refinement around local maxima of |v|
                for k in 1..=OSF {
                    let b = v[k].abs();
                    peak = peak.max(b);
                    let sg = v[k].signum();
                    let (a, c) = (sg * v[k - 1], sg * v[k + 1]);
                    if b >= a && b >= c {
                        let den = a - 2.0 * b + c;
                        if den < 0.0 {
                            peak = peak.max(b - (c - a) * (c - a) / (8.0 * den));
                        }
                    }
                }
            }
            self.hpos = if self.hpos + 1 == PH { 0 } else { self.hpos + 1 };
            #[cfg(test)]
            {
                self.max_est = self.max_est.max(peak);
            }
            // peak estimate refers to sample i - ISP_DELAY; audio delay aligns
            let c = ceil[i] * target;
            // Off: no requirement, the gain releases smoothly back to unity
            let r = if on && peak > c { c / peak } else { 1.0 };
            // 3: sliding min over k+1+2*HOLD
            let nq = self.rq.len();
            self.rq[self.rpos] = r;
            self.rpos = (self.rpos + 1) % nq;
            self.min_age += 1;
            if r <= self.cur_min {
                self.cur_min = r;
                self.min_age = 0;
            } else if self.min_age >= nq {
                let mut m = f64::MAX;
                let mut age = 0;
                for k in 0..nq {
                    let idx = (self.rpos + nq - 1 - k) % nq;
                    if self.rq[idx] < m {
                        m = self.rq[idx];
                        age = k;
                    }
                }
                self.cur_min = m;
                self.min_age = age;
            }
            let na = self.avg.len();
            self.asum += self.cur_min - self.avg[self.apos];
            self.avg[self.apos] = self.cur_min;
            self.apos = (self.apos + 1) % na;
            if self.apos == 0 {
                self.asum = self.avg.iter().sum();
            }
            let a = (self.asum / na as f64).min(1.0);
            // 4: upward-only release, never above the averaged requirement
            self.g = a.min(self.g + (1.0 - self.g) * self.rel);
            if 1.0 - self.g < 1e-12 {
                self.g = 1.0; // exact unity keeps the off/neutral path bit-transparent
            }
            let g = self.g;
            min_g = min_g.min(g);
            for (ch, buf) in chans.iter_mut().enumerate() {
                let d = self.audio[ch].tick(buf[i]);
                buf[i] = d * g;
            }
        }
        self.last_gr_db = 20.0 * min_g.log10();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tp16(y: &[f64]) -> f64 {
        let h = lowpass(16 * 32 + 1, 0.5 / 16.0, 10.0);
        let mut tp: f64 = 0.0;
        for i in 40..y.len() - 40 {
            for ph in 0..16 {
                let mut s = 0.0;
                for k in 0..33 {
                    let idx = 16 * k + ph;
                    if idx < h.len() {
                        s += 16.0 * h[idx] * y[i + 16 - k];
                    }
                }
                tp = tp.max(s.abs());
            }
        }
        tp
    }

    #[test]
    fn isp_guard_on_clipped_noise() {
        let n = 20000;
        let mut s: u64 = 99;
        let x: Vec<f64> = (0..n)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                (4.0 * ((s >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0)).clamp(-1.0, 1.0)
            })
            .collect();
        // band-limit like the decimator output does (pass < 0.4535 fs)
        let lp = lowpass(255, 0.476, kaiser_beta(100.0));
        let x: Vec<f64> = (0..n)
            .map(|i| (0..255).filter(|&k| k <= i).map(|k| lp[k] * x[i - k]).sum())
            .collect();
        let before = tp16(&x);
        {
            // estimator vs reference on the same input, guard disabled
            let mut g = Guard::new(48000.0, 1);
            let mut y = x.clone();
            let ceil = vec![1e-9; n];
            let mut refs: [&mut [f64]; 1] = [&mut y[..]];
            g.process(&mut refs, n, &ceil, GuardMode::Off);
            eprintln!("estimator max {:.3} dB (ref {:.3} dB)", 20.0 * g.max_est.log10(), 20.0 * tp16(&x).log10());
        }
        let mut g = Guard::new(48000.0, 1);
        let mut y = x.clone();
        let ceil = vec![1.0; n];
        {
            let mut refs: [&mut [f64]; 1] = [&mut y[..]];
            g.process(&mut refs, n, &ceil, GuardMode::TruePeak);
        }
        let after = tp16(&y[100..]);
        if let Ok(path) = std::env::var("GUARD_DUMP") {
            let mut b = Vec::new();
            for i in 0..n {
                b.extend_from_slice(&x[i].to_le_bytes());
                b.extend_from_slice(&y[i].to_le_bytes());
            }
            std::fs::write(path, b).unwrap();
        }
        eprintln!("tp before {:.2} dB, after {:.3} dB", 20.0 * before.log10(), 20.0 * after.log10());
        assert!(20.0 * after.log10() < 0.01, "after {}", 20.0 * after.log10());
    }
}
