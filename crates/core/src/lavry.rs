//! Converter features of the original hardware (AD122-96 MkIII manual) that
//! the reference plugin does not model:
//!
//! * Polarity inversion and digital DC removal (input side; the manual's DC
//!   filter settles in under 3 s -> one pole at 0.3 Hz).
//! * Output word length 16..24 bits with the manual's dither/noise-shaping
//!   scheme: "triangle high pass" dither (a new uniform random number minus
//!   the previous one), or flat TPDF; four smooth error-feedback noise
//!   shapers NS1..NS4 traced from the manual's Figs. 11-12 (deep dips near
//!   4 kHz and 12 kHz, noise pushed above 14 kHz; NS1 milder than NS2).
//!   As in the hardware, shaping is off whenever dither is off.
//! * Data limiting: full scale held 1 LSB below the maximum, so downstream
//!   meters never flag an "over".

use crate::converter::Ramp;
use crate::design::minimum_phase;

/// Error-feedback order.
const NS_ORDER: usize = 24;

/// Target noise shapes (dB re flat dither), (Hz, dB), traced from the
/// manual's figures at 44.1 kHz (axis 0..20 kHz). Approximate by nature.
const NS_TARGETS: [[(f64, f64); 13]; 4] = [
    [(0.0, -2.0), (2000.0, -2.5), (4000.0, -5.0), (6000.0, -3.5), (8000.0, -2.0), (10000.0, -2.0), (12000.0, -4.0),
     (13000.0, -3.0), (14000.0, 0.0), (16000.0, 3.0), (18000.0, 5.0), (20000.0, 6.0), (24000.0, 6.0)],
    [(0.0, -5.0), (2000.0, -5.0), (4000.0, -9.0), (6000.0, -6.0), (8000.0, -4.0), (10000.0, -4.0), (12000.0, -6.0),
     (13000.0, -2.0), (14000.0, 2.0), (16000.0, 8.0), (18000.0, 11.0), (20000.0, 12.0), (24000.0, 12.0)],
    [(0.0, -7.0), (2000.0, -8.0), (4000.0, -12.0), (6000.0, -8.0), (8000.0, -5.0), (10000.0, -5.0), (12000.0, -12.0),
     (13000.0, -11.0), (14000.0, -3.0), (16000.0, 11.0), (18000.0, 17.0), (20000.0, 20.0), (24000.0, 20.0)],
    [(0.0, -7.0), (2000.0, -9.0), (4000.0, -23.0), (6000.0, -15.0), (8000.0, -8.0), (10000.0, -8.0), (12000.0, -13.0),
     (13000.0, -11.0), (14000.0, 0.0), (16000.0, 15.0), (18000.0, 25.0), (20000.0, 30.0), (24000.0, 30.0)],
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dither {
    /// "Triangle high pass": r[n] - r[n-1] (the manual's preferred dither).
    HighPass,
    /// Flat TPDF (recommended by the manual for data-compression targets).
    Flat,
}

#[derive(Clone, Copy, Debug)]
pub struct ConverterSettings {
    pub polarity_invert: bool,
    pub dc_removal: bool,
    /// None = 64-bit float output (no word-length reduction).
    pub word_length: Option<u8>,
    pub dither: Option<Dither>,
    /// 0 = off, 1..=4 = NS1..NS4 (ignored when dither is off)
    pub noise_shaping: u8,
    pub data_limit: bool,
}

impl Default for ConverterSettings {
    fn default() -> Self {
        ConverterSettings { polarity_invert: false, dc_removal: false, word_length: None, dither: Some(Dither::HighPass), noise_shaping: 0, data_limit: false }
    }
}

/// Design the error-feedback coefficients c[1..] for shaper `k` (1..=4):
/// noise transfer NTF(z) = 1 + sum c_j z^-j, minimum phase, log-mean 0 dB
/// (the Gerzon-Craven constraint for a unit leading coefficient).
pub fn design_shaper(k: usize, fs: f64) -> Vec<f64> {
    let tgt = &NS_TARGETS[k - 1];
    let n = 1 << 12;
    let db_at = |f: f64| -> f64 {
        // above 20 kHz (high rates) keep the top value: noise goes ultrasonic
        let f = f.min(24000.0);
        for w in tgt.windows(2) {
            if f <= w[1].0 {
                let t = (f - w[0].0) / (w[1].0 - w[0].0);
                return w[0].1 + t * (w[1].1 - w[0].1);
            }
        }
        tgt[tgt.len() - 1].1
    };
    // magnitude on a symmetric FFT grid, normalised to zero log-mean
    let mags: Vec<f64> = (0..n).map(|i| {
        let fi = if i <= n / 2 { i } else { n - i } as f64 * fs / n as f64;
        db_at(fi)
    }).collect();
    let mean_db = mags.iter().sum::<f64>() / n as f64;
    // build a long zero-phase "impulse" with that magnitude, then make it
    // minimum phase and truncate
    let (mut re, mut im) = (mags.iter().map(|d| 10f64.powf((d - mean_db) / 20.0)).collect::<Vec<_>>(), vec![0.0; n]);
    crate::design::fft(&mut re, &mut im, true);
    let mut h: Vec<f64> = (0..n).map(|i| re[(i + n / 2) % n]).collect();
    // window the zero-phase response to a reasonable length before min-phase
    let half = 256;
    let c = n / 2;
    let win: Vec<f64> = h[c - half..c + half].iter().enumerate().map(|(i, v)| {
        let w = 0.5 - 0.5 * (std::f64::consts::PI * 2.0 * i as f64 / (2 * half) as f64).cos();
        v * w
    }).collect();
    h = minimum_phase(&win, NS_ORDER + 1);
    let h0 = h[0];
    h.iter().skip(1).map(|v| v / h0).collect()
}

struct Rng(u64);
impl Rng {
    #[inline]
    fn uni(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

pub struct ConverterIo {
    fs: f64,
    // input side
    dc_r: f64,
    dc_x1: Vec<f64>,
    dc_y1: Vec<f64>,
    pol: Ramp,
    dc_mix: Ramp,
    // output side
    shapers: [Vec<f64>; 4],
    err: Vec<[f64; NS_ORDER]>,
    epos: usize,
    rng: Vec<Rng>,
    r_prev: Vec<f64>,
}

impl ConverterIo {
    pub fn new(fs: f64, channels: usize) -> Self {
        let mut pol = Ramp::new(0.0);
        pol.set_len(0.010 * fs);
        let mut dc_mix = Ramp::new(0.0);
        dc_mix.set_len(0.010 * fs);
        ConverterIo {
            fs,
            dc_r: (-2.0 * std::f64::consts::PI * 0.3 / fs).exp(),
            dc_x1: vec![0.0; channels],
            dc_y1: vec![0.0; channels],
            pol,
            dc_mix,
            shapers: std::array::from_fn(|k| design_shaper(k + 1, fs)),
            err: vec![[0.0; NS_ORDER]; channels],
            epos: 0,
            rng: (0..channels).map(|c| Rng(0x9E37_79B9_7F4A_7C15 ^ (c as u64 + 1) * 0x1234_5678_9ABC)).collect(),
            r_prev: vec![0.0; channels],
        }
    }

    pub fn sample_rate(&self) -> f64 {
        self.fs
    }

    pub fn reset(&mut self) {
        self.dc_x1.iter_mut().for_each(|v| *v = 0.0);
        self.dc_y1.iter_mut().for_each(|v| *v = 0.0);
        self.err.iter_mut().for_each(|e| *e = [0.0; NS_ORDER]);
    }

    pub fn snap(&mut self, s: &ConverterSettings) {
        self.pol.snap(if s.polarity_invert { 1.0 } else { 0.0 });
        self.dc_mix.snap(if s.dc_removal { 1.0 } else { 0.0 });
    }

    pub fn input_active(&self, s: &ConverterSettings) -> bool {
        s.polarity_invert || s.dc_removal || self.pol.v != 0.0 || self.dc_mix.v != 0.0
    }

    /// Polarity and DC removal, in place, all channels (before everything).
    pub fn process_input(&mut self, chans: &mut [&mut [f64]], n: usize, s: &ConverterSettings) {
        let tp = if s.polarity_invert { 1.0 } else { 0.0 };
        let td = if s.dc_removal { 1.0 } else { 0.0 };
        for i in 0..n {
            let p = self.pol.tick(tp);
            let d = self.dc_mix.tick(td);
            let g = 1.0 - 2.0 * p; // crossfades through zero: no click
            for (ch, buf) in chans.iter_mut().enumerate() {
                let x = buf[i];
                let y = x - self.dc_x1[ch] + self.dc_r * self.dc_y1[ch];
                self.dc_x1[ch] = x;
                self.dc_y1[ch] = if y.abs() < 1e-30 { 0.0 } else { y };
                let v = x + d * (y - x);
                buf[i] = g * v;
            }
        }
    }

    pub fn output_active(s: &ConverterSettings) -> bool {
        s.word_length.is_some() || s.data_limit
    }

    /// Word-length reduction (dither + noise shaping) and data limiting,
    /// in place, all channels (the very last stage).
    pub fn process_output(&mut self, chans: &mut [&mut [f64]], n: usize, s: &ConverterSettings) {
        let bits = s.word_length.map(|b| b.clamp(8, 24) as i32);
        let lsb = bits.map(|b| 2f64.powi(-(b - 1)));
        let fs_max = 1.0 - lsb.unwrap_or(2f64.powi(-23));
        let shaping = if s.dither.is_some() { s.noise_shaping.min(4) as usize } else { 0 };
        for i in 0..n {
            for (ch, buf) in chans.iter_mut().enumerate() {
                let x = buf[i];
                let mut y = x;
                if let Some(q) = lsb {
                    // error feedback: v = x + sum c_j e[n-j]
                    let mut v = x;
                    if shaping > 0 {
                        let c = &self.shapers[shaping - 1];
                        let e = &self.err[ch];
                        for (j, cj) in c.iter().enumerate() {
                            v += cj * e[(self.epos + NS_ORDER - 1 - j) % NS_ORDER];
                        }
                    }
                    let d = match s.dither {
                        Some(Dither::HighPass) => {
                            let r = self.rng[ch].uni();
                            let d = r - self.r_prev[ch];
                            self.r_prev[ch] = r;
                            d
                        }
                        Some(Dither::Flat) => self.rng[ch].uni() + self.rng[ch].uni() - 1.0,
                        None => 0.0,
                    };
                    y = ((v / q + d).round() * q).clamp(-1.0, fs_max);
                    // total error, bounded so a clamped overload cannot run away
                    let e = (y - v).clamp(-8.0 * q, 8.0 * q);
                    self.err[ch][self.epos] = e;
                }
                if s.data_limit {
                    y = y.clamp(-fs_max, fs_max);
                }
                buf[i] = y;
            }
            if lsb.is_some() {
                self.epos = (self.epos + 1) % NS_ORDER;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::fft;

    fn ntf_db(c: &[f64], f: f64, fs: f64) -> f64 {
        let (mut re, mut im) = (1.0, 0.0);
        for (j, v) in c.iter().enumerate() {
            let w = 2.0 * std::f64::consts::PI * f / fs * (j + 1) as f64;
            re += v * w.cos();
            im -= v * w.sin();
        }
        10.0 * (re * re + im * im).log10()
    }

    #[test]
    fn shapers_follow_the_manual_figures() {
        let fs = 44100.0;
        for k in 1..=4 {
            let c = design_shaper(k, fs);
            let at4k = ntf_db(&c, 4000.0, fs);
            let at20k = ntf_db(&c, 20000.0, fs);
            let at1k = ntf_db(&c, 1000.0, fs);
            eprintln!("NS{k}: 1k {at1k:.1}  4k {at4k:.1}  20k {at20k:.1} dB");
            assert!(at4k < at1k, "NS{k} needs its 4 kHz dip");
            assert!(at20k > at1k + 8.0, "NS{k} must push noise up");
        }
        let _ = fft;
    }

    #[test]
    fn sixteen_bit_output_is_on_the_grid() {
        let mut c = ConverterIo::new(44100.0, 1);
        let s = ConverterSettings { word_length: Some(16), dither: Some(Dither::HighPass), noise_shaping: 3, ..Default::default() };
        let mut x: Vec<f64> = (0..4410).map(|i| 0.3 * (i as f64 * 0.07).sin()).collect();
        {
            let mut r: [&mut [f64]; 1] = [&mut x[..]];
            c.process_output(&mut r, 4410, &s);
        }
        for v in &x {
            let k = v * 32768.0;
            assert!((k - k.round()).abs() < 1e-9);
        }
    }
}
