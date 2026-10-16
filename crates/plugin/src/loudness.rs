//! Short-term loudness (ITU-R BS.1770 K-weighting, 3 s window, 100 ms hops).
//! Allocation happens only in `new`.

#[derive(Clone, Copy, Default)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    z: [f64; 2],
}

impl Biquad {
    #[inline]
    fn tick(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// K-weighting stage 1 (high shelf) and 2 (high pass), RBJ forms with the
/// BS.1770 analogue parameters, valid at any sample rate.
fn k_weighting(fs: f64) -> [Biquad; 2] {
    use std::f64::consts::PI;
    // shelf: f0 1681.97 Hz, gain +3.9998 dB, Q 0.7072
    let (f0, g, q) = (1681.974450955533, 3.999843853973347, 0.7071752369554196);
    let k = (PI * f0 / fs).tan();
    let vh = 10f64.powf(g / 20.0);
    let vb = vh.powf(0.4996667741545416);
    let a0 = 1.0 + k / q + k * k;
    let shelf = Biquad {
        b: [(vh + vb * k / q + k * k) / a0, 2.0 * (k * k - vh) / a0, (vh - vb * k / q + k * k) / a0],
        a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
        z: [0.0; 2],
    };
    // high pass: f0 38.135 Hz, Q 0.5003
    let (f0, q) = (38.13547087602444, 0.5003270373238773);
    let k = (PI * f0 / fs).tan();
    let a0 = 1.0 + k / q + k * k;
    let hp = Biquad { b: [1.0, -2.0, 1.0], a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0], z: [0.0; 2] };
    [shelf, hp]
}

pub struct ShortTerm {
    filt: [[Biquad; 2]; 2],
    hop: usize,
    count: usize,
    acc: f64,
    bins: Vec<f64>,
    pos: usize,
    filled: usize,
}

impl ShortTerm {
    pub fn new(fs: f64) -> Self {
        let k = k_weighting(fs);
        ShortTerm { filt: [k, k], hop: (fs * 0.1) as usize, count: 0, acc: 0.0, bins: vec![0.0; 30], pos: 0, filled: 0 }
    }

    /// Feed one frame (up to two channels).
    #[inline]
    pub fn push(&mut self, l: f64, r: f64) {
        let mut e = 0.0;
        for (c, x) in [l, r].into_iter().enumerate() {
            let s1 = self.filt[c][0].tick(x);
            let y = self.filt[c][1].tick(s1);
            e += y * y;
        }
        self.acc += e;
        self.count += 1;
        if self.count == self.hop {
            self.bins[self.pos] = self.acc / self.hop as f64;
            self.pos = (self.pos + 1) % self.bins.len();
            self.filled = (self.filled + 1).min(self.bins.len());
            self.acc = 0.0;
            self.count = 0;
        }
    }

    /// Short-term loudness in LUFS (-inf when silent).
    pub fn lufs(&self) -> f64 {
        if self.filled == 0 {
            return f64::NEG_INFINITY;
        }
        let ms: f64 = self.bins.iter().sum::<f64>() / self.filled as f64;
        -0.691 + 10.0 * ms.max(1e-20).log10()
    }
}
