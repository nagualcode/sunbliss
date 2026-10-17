//! Oversampling cascade.
//!
//! Level 1 (base <-> 2x) is the "converter decimation filter": linear phase,
//! flat to max(20 kHz, 0.4167 fs) (capped at 0.4535 fs), stopband from
//! exactly fs/2 with 140 dB rejection, so nothing above Nyquist can alias
//! into the base band (a half-band design, as used by the reference, lets
//! 20..24 kHz alias). At 96 kHz this keeps the band flat to 40 kHz.
//! Levels >= 2 are short half-bands whose transition sits far above audio.
//!
//! The non-linear stages run second-order ADAA, which delays by one top-rate
//! sample and, in its linear region, acts as the 3-tap mean
//! (1 + 2 cos w)/3. The level-1 decimator is designed with the inverse of
//! that roll-off built in, so the whole wet path is flat and an integer delay.

use crate::design::{halfband, kaiser_beta, kaiser_len, lowpass, lowpass_shaped, minimum_phase};
use crate::fir::{Down2, Up2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    High,
    Pristine,
    Extra,
}

impl Quality {
    pub fn from_index(i: usize) -> Self {
        match i {
            0 => Quality::High,
            1 => Quality::Pristine,
            _ => Quality::Extra,
        }
    }
    /// Oversampling factor at sample rate `fs` (same table as the reference).
    pub fn factor(self, fs: f64) -> usize {
        let base = match self {
            Quality::High => 4,
            Quality::Pristine => 8,
            Quality::Extra => 16,
        };
        if fs <= 50_000.0 {
            base
        } else if fs <= 100_000.0 {
            base / 2
        } else {
            (base / 4).max(1)
        }
    }
}

/// Group delay at DC (first moment); exactly (N-1)/2 for linear phase.
fn dc_delay(h: &[f64]) -> f64 {
    let s: f64 = h.iter().sum();
    let m: f64 = h.iter().enumerate().map(|(i, v)| i as f64 * v).sum();
    let d = m / s;
    // snap the linear-phase case to its exact half-integer value
    let lin = (h.len() - 1) as f64 / 2.0;
    if (d - lin).abs() < 1e-6 { lin } else { d }
}

pub const PASS_EDGE_MAX: f64 = 0.4535; // of fs
pub const PASS_EDGE_MIN: f64 = 0.4167; // of fs
pub const PASS_HZ: f64 = 20_000.0;

/// Pass-band edge of the converter filter, as a fraction of fs.
pub fn pass_edge(fs: f64) -> f64 {
    (PASS_HZ / fs).clamp(PASS_EDGE_MIN, PASS_EDGE_MAX)
}
const ATTEN_DB: f64 = 140.0;

pub struct Oversampler {
    k: usize,
    up: Vec<Up2>,
    dn: Vec<Down2>,
    /// lvl[i] holds signal at rate 2^i fs (lvl[0] unused scratch).
    lvl: Vec<Vec<f64>>,
    adaa: bool,
    latency_top: f64,
    latency_up_top: f64,
}

impl Oversampler {
    pub fn new(fs: f64, factor: usize, max_block: usize, min_phase: bool) -> Self {
        assert!(factor.is_power_of_two());
        let k = factor.trailing_zeros() as usize;
        let adaa = k >= 1;
        let beta = kaiser_beta(ATTEN_DB);
        let mut up = Vec::new();
        let mut dn = Vec::new();
        let mut lat = 0.0;
        let mut lat_up = 0.0;
        for lv in 1..=k {
            let max_in = max_block << (lv - 1);
            let (hu, hd) = if lv == 1 {
                // converter stage at rate 2 fs
                let fp = pass_edge(fs) / 2.0;
                let fstop = 0.25;
                let mut n = kaiser_len(ATTEN_DB, fstop - fp);
                if n % 2 == 0 {
                    n += 1;
                }
                let fc = 0.5 * (fp + fstop);
                let hu = lowpass(n, fc, beta);
                let comp_l = factor as f64;
                let hd = lowpass_shaped(n, fc, beta, |f| {
                    if adaa {
                        let w = 2.0 * std::f64::consts::PI * (2.0 * f / comp_l);
                        3.0 / (1.0 + 2.0 * w.cos())
                    } else {
                        1.0
                    }
                });
                (hu, hd)
            } else {
                let p = 0.5 / (1u64 << lv) as f64;
                let dw = 0.5 - 2.0 * p;
                let mut n = kaiser_len(ATTEN_DB, dw).max(7);
                while n % 4 != 3 {
                    n += 1;
                }
                let hd = halfband(n, beta);
                (hd.clone(), hd)
            };
            let (hu, hd) = if min_phase { (minimum_phase(&hu, hu.len()), minimum_phase(&hd, hd.len())) } else { (hu, hd) };
            let w = (1usize << (k - lv)) as f64;
            lat += dc_delay(&hu) * w;
            lat_up += dc_delay(&hu) * w;
            lat += dc_delay(&hd) * w;
            up.push(Up2::new(&hu, max_in));
            dn.push(Down2::new(&hd, max_in * 2));
        }
        if adaa {
            lat += 1.0;
        }
        let lvl = (0..=k).map(|lv| vec![0.0; max_block << lv]).collect();
        Oversampler { k, up, dn, lvl, adaa, latency_top: lat, latency_up_top: lat_up }
    }

    pub fn factor(&self) -> usize {
        1 << self.k
    }
    pub fn uses_adaa(&self) -> bool {
        self.adaa
    }
    /// Round-trip delay in top-rate samples, including the ADAA2 sample.
    pub fn latency_top(&self) -> f64 {
        self.latency_top
    }

    /// Delay of the interpolation chain alone, in top-rate samples.
    pub fn latency_up_top(&self) -> f64 {
        self.latency_up_top
    }

    pub fn reset(&mut self) {
        self.up.iter_mut().for_each(|s| s.reset());
        self.dn.iter_mut().for_each(|s| s.reset());
    }

    /// Upsample `inp` and return the top-rate buffer (len `inp.len() * L`).
    pub fn up(&mut self, inp: &[f64]) -> &mut [f64] {
        let n = inp.len();
        if self.k == 0 {
            self.lvl[0][..n].copy_from_slice(inp);
            return &mut self.lvl[0][..n];
        }
        let (first, rest) = self.lvl.split_at_mut(1);
        let _ = first;
        self.up[0].process(inp, &mut rest[0][..2 * n]);
        for lv in 2..=self.k {
            let (a, b) = self.lvl.split_at_mut(lv);
            let m = n << (lv - 1);
            self.up[lv - 1].process(&a[lv - 1][..m], &mut b[0][..2 * m]);
        }
        &mut self.lvl[self.k][..n << self.k]
    }

    /// Top-rate buffer for `n` base samples (to process in place after `up`).
    pub fn top(&mut self, n: usize) -> &mut [f64] {
        &mut self.lvl[self.k][..n << self.k]
    }

    /// Decimate the top-rate buffer (as left by the caller) into `out`.
    pub fn down(&mut self, out: &mut [f64]) {
        let n = out.len();
        if self.k == 0 {
            out.copy_from_slice(&self.lvl[0][..n]);
            return;
        }
        for lv in (2..=self.k).rev() {
            let (a, b) = self.lvl.split_at_mut(lv);
            let m = n << lv;
            self.dn[lv - 1].process(&b[0][..m], &mut a[lv - 1][..m / 2]);
        }
        let (a, b) = self.lvl.split_at_mut(1);
        let _ = a;
        self.dn[0].process(&b[0][..2 * n], out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(factor: usize) {
        let mut os = Oversampler::new(48000.0, factor, 512, false);
        let lat = os.latency_top();
        let l = factor as f64;
        // the identity path at the top averages adjacent samples (ADAA identity)
        let n = 512;
        let mut prev = [0.0f64; 2];
        assert!(lat.fract().abs() < 1e-9, "top latency not integer: {lat}");
        let lat_top = lat.round() as usize;
        let pad = (factor - lat_top % factor) % factor;
        let mut pad_line = crate::fir::Delay::new(pad.max(1));
        pad_line.set(pad);
        let mut y_all = Vec::new();
        let x_all: Vec<f64> = (0..n * 16).map(|i| 0.5 * (i as f64 * 0.01).sin() + 0.2 * (i as f64 * 0.7).sin()).collect();
        for blk in x_all.chunks(n) {
            let top = os.up(blk);
            for v in top.iter_mut() {
                *v = pad_line.tick(*v);
            }
            if factor > 1 {
                for v in top.iter_mut() {
                    let cur = *v;
                    *v = (cur + prev[0] + prev[1]) / 3.0;
                    prev = [cur, prev[0]];
                }
            }
            let mut y = vec![0.0; n];
            os.down(&mut y);
            y_all.extend_from_slice(&y);
        }
        let d = (lat_top + pad) / factor;
        let _ = l;
        let mut err: f64 = 0.0;
        for i in 4 * n..y_all.len() {
            err = err.max((y_all[i] - x_all[i - d]).abs());
        }
        // at 2x the ADAA2 roll-off compensation reaches +5.9 dB at the band
        // edge, which costs some ripple (-71 dB); higher factors stay < 2e-4
        let tol = if factor == 2 { 5e-4 } else { 2e-4 };
        assert!(err < tol, "factor {factor}: err {err}, latency {d}");
    }

    #[test]
    fn roundtrip_all_factors() {
        for f in [1, 2, 4, 8, 16] {
            roundtrip(f);
        }
    }
}

#[cfg(test)]
mod minphase_latency {
    use super::*;
    #[test]
    fn min_phase_has_less_latency() {
        for f in [2, 4, 8] {
            let a = Oversampler::new(48000.0, f, 64, false).latency_top();
            let b = Oversampler::new(48000.0, f, 64, true).latency_top();
            eprintln!("factor {f}: linear {a} top samples, minimum {b}");
            assert!(b < a / 3.0);
        }
    }
}
