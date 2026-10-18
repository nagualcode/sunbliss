//! Polyphase 2x interpolation / decimation stages.
//!
//! All buffers are sized at construction; `process` never allocates.

#[inline]
pub(crate) fn dot(a: &[f64], b: &[f64]) -> f64 {
    // 8 independent lanes over exact chunks: LLVM turns this into paired
    // 2-wide FMAs (NEON) / 4-wide (AVX) without bounds checks.
    let n = a.len().min(b.len());
    let (a, b) = (&a[..n], &b[..n]);
    let mut acc = [0.0f64; 8];
    let ca = a.chunks_exact(8);
    let cb = b.chunks_exact(8);
    let (ra, rb) = (ca.remainder(), cb.remainder());
    for (x, y) in ca.zip(cb) {
        for k in 0..8 {
            acc[k] = x[k].mul_add(y[k], acc[k]);
        }
    }
    let mut s = ((acc[0] + acc[1]) + (acc[2] + acc[3])) + ((acc[4] + acc[5]) + (acc[6] + acc[7]));
    for (x, y) in ra.iter().zip(rb) {
        s = x.mul_add(*y, s);
    }
    s
}

/// Dot product with a symmetric kernel c (c[k] == c[t-1-k]), given as its
/// first half `half` (and the centre tap `mid` when t is odd): the window is
/// folded first, so a t-tap linear-phase FIR costs ~t/2 multiplies.
#[inline]
pub(crate) fn dot_sym(half: &[f64], mid: f64, w: &[f64]) -> f64 {
    let h = half.len();
    let t = w.len();
    let (front, back) = (&w[..h], &w[t - h..]);
    let mut acc = [0.0f64; 8];
    let cf = front.chunks_exact(8);
    let cb = back.rchunks_exact(8);
    let ch = half.chunks_exact(8);
    let (rf, rb, rh) = (cf.remainder(), cb.remainder(), ch.remainder());
    for ((x, y), c) in cf.zip(cb).zip(ch) {
        for k in 0..8 {
            acc[k] = c[k].mul_add(x[k] + y[7 - k], acc[k]);
        }
    }
    let mut s = ((acc[0] + acc[1]) + (acc[2] + acc[3])) + ((acc[4] + acc[5]) + (acc[6] + acc[7]));
    // remainders: rf is the tail of the front half, rb the head of the back half
    let r = rh.len();
    for k in 0..r {
        s = rh[k].mul_add(rf[k] + rb[r - 1 - k], s);
    }
    if t % 2 == 1 {
        s = mid.mul_add(w[h], s);
    }
    s
}

/// A dense kernel, folded when it is symmetric (linear phase).
struct Taps {
    full: Vec<f64>,
    /// Some((first half, centre)) when symmetric
    sym: Option<(Vec<f64>, f64)>,
}

impl Taps {
    fn new(c: Vec<f64>) -> Self {
        let t = c.len();
        let peak = c.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let symmetric = t >= 16 && (0..t / 2).all(|k| (c[k] - c[t - 1 - k]).abs() <= 1e-12 * peak);
        let sym = symmetric.then(|| {
            let half = (0..t / 2).map(|k| 0.5 * (c[k] + c[t - 1 - k])).collect();
            (half, if t % 2 == 1 { c[t / 2] } else { 0.0 })
        });
        Taps { full: c, sym }
    }
    fn len(&self) -> usize {
        self.full.len()
    }
    #[inline]
    fn dot(&self, w: &[f64]) -> f64 {
        match &self.sym {
            Some((half, mid)) => dot_sym(half, *mid, w),
            None => dot(&self.full, w),
        }
    }
}

enum Kernel {
    /// Reversed dense coefficients.
    Dense(Taps),
    /// A single non-zero tap at reversed position `pos`.
    Single { pos: usize, gain: f64 },
    /// Taps at even reversed positions (compacted) plus one extra tap.
    HalfBand { even: Taps, center_pos: usize, center: f64 },
}

/// 2x interpolator: `n` input samples -> `2n` output samples.
pub struct Up2 {
    phases: [Kernel; 2],
    hist: usize,
    buf: Vec<f64>,
    /// Group delay in output-rate samples.
    pub delay: f64,
}

impl Up2 {
    pub fn new(h: &[f64], max_in: usize) -> Self {
        let n = h.len();
        let t = n.div_ceil(2);
        let mk = |p: usize| -> Kernel {
            let mut c: Vec<f64> = (0..t).map(|j| h.get(2 * j + p).copied().unwrap_or(0.0) * 2.0).collect();
            c.reverse();
            let nz: Vec<usize> = (0..t).filter(|&i| c[i] != 0.0).collect();
            if nz.len() == 1 {
                Kernel::Single { pos: nz[0], gain: c[nz[0]] }
            } else {
                Kernel::Dense(Taps::new(c))
            }
        };
        Up2 { phases: [mk(0), mk(1)], hist: t - 1, buf: vec![0.0; t - 1 + max_in], delay: (n - 1) as f64 / 2.0 }
    }

    pub fn reset(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 0.0);
    }

    pub fn process(&mut self, inp: &[f64], out: &mut [f64]) {
        let n = inp.len();
        debug_assert!(out.len() >= 2 * n);
        let h = self.hist;
        self.buf[h..h + n].copy_from_slice(inp);
        for (p, k) in self.phases.iter().enumerate() {
            match k {
                Kernel::Dense(c) => {
                    let t = c.len();
                    for i in 0..n {
                        out[2 * i + p] = c.dot(&self.buf[i..i + t]);
                    }
                }
                Kernel::Single { pos, gain } => {
                    for i in 0..n {
                        out[2 * i + p] = gain * self.buf[i + pos];
                    }
                }
                Kernel::HalfBand { .. } => unreachable!(),
            }
        }
        self.buf.copy_within(n..n + h, 0);
    }
}

/// 2x decimator: `2n` input samples -> `n` output samples.
pub struct Down2 {
    kernel: Kernel,
    hist: usize,
    buf: Vec<f64>,
    /// half-band only: the even-position samples of `buf`, contiguous
    ev: Vec<f64>,
    /// Group delay in input-rate samples.
    pub delay: f64,
}

impl Down2 {
    pub fn new(h: &[f64], max_in: usize) -> Self {
        let n = h.len();
        let mut r = h.to_vec();
        r.reverse();
        let nz: Vec<usize> = (0..n).filter(|&i| r[i] != 0.0).collect();
        let odd_nz: Vec<usize> = nz.iter().copied().filter(|i| i % 2 == 1).collect();
        let kernel = if n % 4 == 3 && odd_nz.len() == 1 {
            // half-band: all non-zero taps at even reversed positions, plus centre
            let even: Vec<f64> = (0..n.div_ceil(2)).map(|j| r.get(2 * j).copied().unwrap_or(0.0)).collect();
            Kernel::HalfBand { even: Taps::new(even), center_pos: odd_nz[0], center: r[odd_nz[0]] }
        } else {
            Kernel::Dense(Taps::new(r))
        };
        let ev = if matches!(kernel, Kernel::HalfBand { .. }) { vec![0.0; (n - 1 + max_in).div_ceil(2)] } else { Vec::new() };
        Down2 { kernel, hist: n - 1, buf: vec![0.0; n - 1 + max_in], ev, delay: (n - 1) as f64 / 2.0 }
    }

    pub fn reset(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 0.0);
    }

    pub fn process(&mut self, inp: &[f64], out: &mut [f64]) {
        let m = inp.len();
        debug_assert!(m % 2 == 0);
        let n = m / 2;
        let h = self.hist;
        self.buf[h..h + m].copy_from_slice(inp);
        match &self.kernel {
            Kernel::Dense(c) => {
                let t = c.len();
                for i in 0..n {
                    out[i] = c.dot(&self.buf[2 * i..2 * i + t]);
                }
            }
            Kernel::HalfBand { even, center_pos, center } => {
                // de-interleave once per block so each output is a contiguous dot
                let ne = (h + m).div_ceil(2);
                for (k, v) in self.ev[..ne].iter_mut().enumerate() {
                    *v = self.buf[2 * k];
                }
                let e = even.len();
                for i in 0..n {
                    out[i] = even.dot(&self.ev[i..i + e]) + center * self.buf[2 * i + center_pos];
                }
            }
            Kernel::Single { pos, gain } => {
                for i in 0..n {
                    out[i] = gain * self.buf[2 * i + pos];
                }
            }
        }
        self.buf.copy_within(m..m + h, 0);
    }
}

/// Plain integer delay line (ring buffer).
pub struct Delay {
    buf: Vec<f64>,
    pos: usize,
    d: usize,
}

impl Delay {
    pub fn new(max_delay: usize) -> Self {
        Delay { buf: vec![0.0; max_delay + 1], pos: 0, d: 0 }
    }
    pub fn set(&mut self, d: usize) {
        assert!(d < self.buf.len());
        self.d = d;
    }
    pub fn delay(&self) -> usize {
        self.d
    }
    pub fn reset(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 0.0);
    }
    #[inline]
    pub fn tick(&mut self, x: f64) -> f64 {
        if self.d == 0 {
            return x;
        }
        let len = self.buf.len();
        self.buf[self.pos] = x;
        // no integer division on the audio path
        let r = if self.pos >= self.d { self.pos - self.d } else { self.pos + len - self.d };
        self.pos += 1;
        if self.pos == len {
            self.pos = 0;
        }
        self.buf[r]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::*;

    #[test]
    fn up_down_halfband_is_near_identity() {
        let h = halfband(39, kaiser_beta(140.0));
        let mut up = Up2::new(&h, 256);
        let mut dn = Down2::new(&h, 512);
        let x: Vec<f64> = (0..256).map(|i| (i as f64 * 0.05).sin() * 0.5).collect();
        let mut u = vec![0.0; 512];
        let mut y = vec![0.0; 256];
        for _ in 0..4 {
            up.process(&x, &mut u);
            dn.process(&u, &mut y);
        }
        // total delay: (39-1)/2 at 2x each way = 38 samples at 2x = 19 base
        let d = 19;
        // skip the region that still sees the previous (discontinuous) block
        for i in 2 * d..256 {
            assert!((y[i] - x[i - d]).abs() < 1e-6, "{} {} {}", i, y[i], x[i - d]);
        }
    }

    #[test]
    fn delay_line() {
        let mut d = Delay::new(8);
        d.set(3);
        let out: Vec<f64> = (1..=6).map(|i| d.tick(i as f64)).collect();
        assert_eq!(out, vec![0.0, 0.0, 0.0, 1.0, 2.0, 3.0]);
    }
}
