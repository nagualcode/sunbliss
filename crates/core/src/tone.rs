//! Tone: subtle converter-style top-end contour, linear phase, base rate.
//!
//! Flat is an exact delay (bit-transparent). A and B are FIRs designed by
//! frequency sampling from the target curves in `curve_db`. Switching modes
//! crossfades the two outputs over 10 ms.

use crate::converter::Ramp;
use crate::design::{freq_sampled, kaiser_beta};
use crate::fir::Delay;

pub const TAPS: usize = 255;
pub const DELAY: usize = (TAPS - 1) / 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToneMode {
    Flat,
    A,
    B,
}

impl ToneMode {
    pub fn from_index(i: usize) -> Self {
        match i {
            0 => ToneMode::Flat,
            1 => ToneMode::A,
            _ => ToneMode::B,
        }
    }
    fn idx(self) -> usize {
        self as usize
    }
}

use crate::tone_tables::*;

/// Measured reference curves exist for three rate families. Within a family
/// the top of the curve (the "hyper-sonic" low-pass) scales with fs, the
/// contour below 20 kHz stays in Hz.
fn table(mode: ToneMode, fs: f64) -> (&'static [(f64, f64)], f64) {
    match (mode, fs) {
        (ToneMode::A, f) if f < 46_000.0 => (&T44100_A, 44_100.0),
        (ToneMode::B, f) if f < 46_000.0 => (&T44100_B, 44_100.0),
        (ToneMode::A, f) if f < 70_000.0 => (&T48000_A, 48_000.0),
        (ToneMode::B, f) if f < 70_000.0 => (&T48000_B, 48_000.0),
        (ToneMode::A, _) => (&T96000_A, 96_000.0),
        _ => (&T96000_B, 96_000.0),
    }
}

fn interp_db(tab: &[(f64, f64)], f: f64) -> f64 {
    if f <= tab[0].0 {
        return tab[0].1;
    }
    for w in tab.windows(2) {
        let ((f0, d0), (f1, d1)) = (w[0], w[1]);
        if f <= f1 {
            let t = (f.ln() - f0.ln()) / (f1.ln() - f0.ln());
            return d0 + t * (d1 - d0);
        }
    }
    // beyond the last measured point: keep falling (4th order per octave)
    let (fl, dl) = tab[tab.len() - 1];
    dl - 24.0 * (f / fl).log2()
}

/// Target magnitude (dB) of a tone mode at frequency `f` Hz, sample rate `fs`.
pub fn curve_db(mode: ToneMode, f: f64, fs: f64) -> f64 {
    if mode == ToneMode::Flat {
        return 0.0;
    }
    let (tab, fs_ref) = table(mode, fs);
    // frequencies above 20 kHz follow the rate (low-pass scales with fs)
    let fm = if f > 20_000.0 { 20_000.0 + (f - 20_000.0) * (fs_ref / 2.0 - 20_000.0) / (fs / 2.0 - 20_000.0).max(1.0) } else { f };
    let fm = if (fs - fs_ref).abs() < 1.0 { f } else { fm };
    interp_db(tab, fm).max(-60.0)
}

pub struct Tone {
    taps: [Vec<f64>; 2], // A, B (reversed)
    hist: Vec<Vec<f64>>, // per channel ring of TAPS (linear buffer with history)
    flat: Vec<Delay>,
    w: [Ramp; 3],
}

impl Tone {
    pub fn new(fs: f64, channels: usize, max_block: usize) -> Self {
        let beta = kaiser_beta(70.0);
        let design = |m: ToneMode| {
            let mut h = freq_sampled(TAPS, beta, |fc| 10f64.powf(curve_db(m, fc * fs, fs) / 20.0));
            h.reverse();
            h
        };
        let mut flat: Vec<Delay> = (0..channels).map(|_| Delay::new(DELAY)).collect();
        flat.iter_mut().for_each(|d| d.set(DELAY));
        let mut w = [Ramp::new(1.0), Ramp::new(0.0), Ramp::new(0.0)];
        w.iter_mut().for_each(|r| r.set_len(0.010 * fs));
        Tone {
            taps: [design(ToneMode::A), design(ToneMode::B)],
            hist: vec![vec![0.0; TAPS - 1 + max_block]; channels],
            flat,
            w,
        }
    }

    pub fn reset(&mut self) {
        self.hist.iter_mut().for_each(|h| h.iter_mut().for_each(|v| *v = 0.0));
        self.flat.iter_mut().for_each(|d| d.reset());
    }

    pub fn snap(&mut self, m: ToneMode) {
        for (i, r) in self.w.iter_mut().enumerate() {
            r.snap(if i == m.idx() { 1.0 } else { 0.0 });
        }
    }

    /// Ramps are shared across channels: call `weights` once per sample
    /// index via `process_all`.
    pub fn process_all(&mut self, chans: &mut [&mut [f64]], n: usize, mode: ToneMode) {
        let h = TAPS - 1;
        for (ch, buf) in chans.iter().enumerate() {
            self.hist[ch][h..h + n].copy_from_slice(&buf[..n]);
        }
        let mi = mode.idx();
        for i in 0..n {
            let ws: [f64; 3] = std::array::from_fn(|m| self.w[m].tick(if m == mi { 1.0 } else { 0.0 }));
            for (ch, buf) in chans.iter_mut().enumerate() {
                let flat = self.flat[ch].tick(buf[i]);
                if ws[0] == 1.0 {
                    buf[i] = flat;
                    continue;
                }
                let x = &self.hist[ch][i..i + TAPS];
                let mut y = ws[0] * flat;
                for m in 0..2 {
                    if ws[m + 1] > 0.0 {
                        let t = &self.taps[m];
                        let mut s = 0.0;
                        for k in 0..TAPS {
                            s += t[k] * x[k];
                        }
                        y += ws[m + 1] * s;
                    }
                }
                buf[i] = y;
            }
        }
        for hbuf in self.hist.iter_mut() {
            hbuf.copy_within(n..n + h, 0);
        }
    }
}
