//! FIR design helpers. Called only at prepare time (allocates).

use std::f64::consts::PI;

/// Modified Bessel function of the first kind, order 0 (series).
pub fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let q = x * x / 4.0;
    let mut k = 1.0;
    while term > 1e-17 * sum {
        term *= q / (k * k);
        sum += term;
        k += 1.0;
    }
    sum
}

/// Kaiser beta for a given stopband attenuation in dB.
pub fn kaiser_beta(atten_db: f64) -> f64 {
    if atten_db > 50.0 {
        0.1102 * (atten_db - 8.7)
    } else if atten_db >= 21.0 {
        0.5842 * (atten_db - 21.0).powf(0.4) + 0.07886 * (atten_db - 21.0)
    } else {
        0.0
    }
}

/// Kaiser taps estimate for transition width `dw` (cycles/sample).
pub fn kaiser_len(atten_db: f64, dw: f64) -> usize {
    ((atten_db - 7.95) / (2.285 * 2.0 * PI * dw)).ceil() as usize + 1
}

pub fn kaiser_window(n: usize, beta: f64) -> Vec<f64> {
    let m = (n - 1) as f64;
    let d = bessel_i0(beta);
    (0..n)
        .map(|i| {
            let r = if m == 0.0 { 0.0 } else { 2.0 * i as f64 / m - 1.0 };
            bessel_i0(beta * (1.0 - r * r).max(0.0).sqrt()) / d
        })
        .collect()
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 { 1.0 } else { (PI * x).sin() / (PI * x) }
}

/// Windowed-sinc lowpass, `cutoff` in cycles/sample (0..0.5), unity DC gain.
/// Works for odd and even `n` (even `n` gives a half-sample group delay).
pub fn lowpass(n: usize, cutoff: f64, beta: f64) -> Vec<f64> {
    let w = kaiser_window(n, beta);
    let c = (n - 1) as f64 / 2.0;
    let mut h: Vec<f64> = (0..n)
        .map(|i| 2.0 * cutoff * sinc(2.0 * cutoff * (i as f64 - c)) * w[i])
        .collect();
    let s: f64 = h.iter().sum();
    h.iter_mut().for_each(|v| *v /= s);
    h
}

/// Half-band lowpass (cutoff 1/4) of length 4m+3 with exact structural zeros.
pub fn halfband(n: usize, beta: f64) -> Vec<f64> {
    assert!(n % 4 == 3, "half-band length must be 4m+3");
    let mut h = lowpass(n, 0.25, beta);
    let c = (n - 1) / 2;
    for (i, v) in h.iter_mut().enumerate() {
        let d = i as isize - c as isize;
        if d == 0 {
            *v = 0.5;
        } else if d % 2 == 0 {
            *v = 0.0;
        }
    }
    // renormalise the odd taps so that DC gain is exactly 1
    let odd: f64 = h.iter().enumerate().filter(|(i, _)| *i != c).map(|(_, v)| v).sum();
    for (i, v) in h.iter_mut().enumerate() {
        if i != c && *v != 0.0 {
            *v *= 0.5 / odd;
        }
    }
    h
}

/// Linear-phase FIR by windowed frequency sampling of a real, zero-phase
/// magnitude `desired(f)` (f in cycles/sample, 0..0.5).
pub fn freq_sampled(n: usize, beta: f64, desired: impl Fn(f64) -> f64) -> Vec<f64> {
    let grid = 1 << 14;
    let df = 0.5 / grid as f64;
    let d: Vec<f64> = (0..=grid).map(|k| desired(k as f64 * df)).collect();
    let w = kaiser_window(n, beta);
    let c = (n - 1) as f64 / 2.0;
    (0..n)
        .map(|i| {
            let m = i as f64 - c;
            // h(m) = 2 * integral_0^0.5 D(f) cos(2 pi f m) df  (trapezoid)
            let mut acc = 0.0;
            for (k, dk) in d.iter().enumerate() {
                let wt = if k == 0 || k == grid { 0.5 } else { 1.0 };
                acc += wt * dk * (2.0 * PI * k as f64 * df * m).cos();
            }
            2.0 * acc * df * w[i]
        })
        .collect()
}

/// Linear-phase low-pass whose pass band follows a smooth gain `comp(f)` up
/// to `fc` (cycles/sample) and is zero above. The integral is taken only over
/// [0, fc] with Simpson's rule, so the cutoff step adds no quadrature error
/// (a uniform grid across the step leaves ~3e-5 of pass-band ripple).
pub fn lowpass_shaped(n: usize, fc: f64, beta: f64, comp: impl Fn(f64) -> f64) -> Vec<f64> {
    let w = kaiser_window(n, beta);
    let c = (n - 1) as f64 / 2.0;
    let k = 8192; // even number of Simpson intervals
    let df = fc / k as f64;
    let g: Vec<f64> = (0..=k).map(|j| comp(j as f64 * df)).collect();
    let mut h: Vec<f64> = (0..n)
        .map(|i| {
            let m = i as f64 - c;
            let mut acc = 0.0;
            for (j, gj) in g.iter().enumerate() {
                let wt = if j == 0 || j == k { 1.0 } else if j % 2 == 1 { 4.0 } else { 2.0 };
                acc += wt * gj * (2.0 * PI * j as f64 * df * m).cos();
            }
            2.0 * acc * df / 3.0 * w[i]
        })
        .collect();
    // exact unity at DC relative to comp(0)
    let s: f64 = h.iter().sum();
    let g0 = comp(0.0);
    h.iter_mut().for_each(|v| *v *= g0 / s);
    h
}

/// Magnitude of an FIR at frequency f (cycles/sample).
pub fn mag_at(h: &[f64], f: f64) -> f64 {
    let (mut re, mut im) = (0.0, 0.0);
    for (i, v) in h.iter().enumerate() {
        let ph = 2.0 * PI * f * i as f64;
        re += v * ph.cos();
        im -= v * ph.sin();
    }
    (re * re + im * im).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halfband_is_flat_and_rejects() {
        let h = halfband(39, kaiser_beta(140.0));
        assert!((mag_at(&h, 0.0) - 1.0).abs() < 1e-12);
        assert!((mag_at(&h, 0.125) - 1.0).abs() < 1e-6);
        assert!(mag_at(&h, 0.375) < 1e-6);
    }

    #[test]
    fn lowpass_even_has_unity_dc() {
        let h = lowpass(40, 0.25, 12.0);
        assert!((mag_at(&h, 0.0) - 1.0).abs() < 1e-12);
    }
}
