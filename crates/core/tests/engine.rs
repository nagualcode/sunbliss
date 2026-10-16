use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use sunbliss_core::{ClipMode, Curve, Engine, Params, Quality, ToneMode};

// ---- allocation counter (only counts while ARMED, on this thread's process calls)
struct Counting;
thread_local! {
    static ARMED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if ARMED.with(|a| a.get()) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}
#[global_allocator]
static GA: Counting = Counting;

fn noise(n: usize, seed: u64, amp: f64) -> Vec<f64> {
    let mut s = seed;
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            amp * ((s >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0)
        })
        .collect()
}

fn run(e: &mut Engine, x: &[Vec<f64>], block: usize) -> Vec<Vec<f64>> {
    let ch = x.len();
    let n = x[0].len();
    let mut out = vec![Vec::with_capacity(n); ch];
    let mut bufs = vec![vec![0.0; block]; ch];
    let mut p = 0;
    while p < n {
        let m = block.min(n - p);
        for c in 0..ch {
            bufs[c][..m].copy_from_slice(&x[c][p..p + m]);
        }
        {
            let mut refs: Vec<&mut [f64]> = bufs.iter_mut().map(|b| &mut b[..m]).collect();
            e.process(&mut refs, m);
        }
        for c in 0..ch {
            out[c].extend_from_slice(&bufs[c][..m]);
        }
        p += m;
    }
    out
}

fn engine(fs: f64, p: Params) -> Engine {
    let mut e = Engine::new(fs, 2, 512);
    e.set_params(&p);
    e.snap();
    e.reset();
    e
}

#[test]
fn neutral_is_bit_transparent() {
    for fs in [44100.0, 48000.0, 96000.0] {
        for q in [Quality::High, Quality::Pristine, Quality::Extra] {
            let mut e = engine(fs, Params { quality: q, ..Params::neutral() });
            let lat = e.latency();
            let x = vec![noise(20000, 1, 0.9), noise(20000, 2, 0.9)];
            let y = run(&mut e, &x, 333);
            for c in 0..2 {
                for i in lat..20000 {
                    assert_eq!(y[c][i], x[c][i - lat], "fs {fs} q {q:?} ch {c} i {i}");
                }
            }
        }
    }
}

#[test]
fn delta_nulls_when_neutral() {
    let mut e = engine(48000.0, Params { delta: true, input_db: 3.0, ..Params::neutral() });
    let x = vec![noise(20000, 3, 0.5), noise(20000, 4, 0.5)];
    let y = run(&mut e, &x, 256);
    let m = y.iter().flat_map(|c| c[2000..].iter()).fold(0.0f64, |a, v| a.max(v.abs()));
    assert!(m == 0.0 || 20.0 * m.log10() < -140.0, "delta residual {m}");
}

#[test]
fn parallel_dry_is_sample_aligned() {
    // clip on but far below the ceiling, wet+dry 50/50 vs dry only: the wet
    // path must be the same delayed signal to within filter flatness
    let p = Params { clip_mode: ClipMode::Hard, link: false, ..Default::default() };
    let mut e = engine(48000.0, p);
    let lat = e.latency();
    let mut x = vec![vec![0.0; 8192]; 2];
    x[0][1000] = 0.25;
    x[1][1000] = 0.25;
    let y = run(&mut e, &x, 512);
    let (imax, vmax) = y[0].iter().enumerate().fold((0, 0.0f64), |a, (i, v)| if v.abs() > a.1 { (i, v.abs()) } else { a });
    assert_eq!(imax, 1000 + lat, "peak at {imax}, expected {}", 1000 + lat);
    assert!((vmax - 0.25).abs() < 0.03, "peak value {vmax}");
}

#[test]
fn transparent_below_ceiling_with_clipper_on() {
    for mode in [ClipMode::Hard, ClipMode::ConverterA] {
        let p = Params { clip_mode: mode, link: false, ..Default::default() };
        let mut e = engine(48000.0, p);
        let lat = e.latency();
        let fs = 48000.0;
        let x: Vec<f64> = (0..48000).map(|i| 0.5 * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / fs).sin()).collect();
        let y = run(&mut e, &[x.clone(), x.clone()].to_vec(), 512);
        let mut err: f64 = 0.0;
        for i in 10000..48000 {
            err = err.max((y[0][i] - x[i - lat]).abs());
        }
        assert!(20.0 * err.log10() < -90.0, "{mode:?}: residual {} dB", 20.0 * err.log10());
    }
}

#[test]
fn robust_under_extremes_and_automation() {
    let mut e = Engine::new(44100.0, 2, 512);
    let mut p = Params::default();
    let x = vec![noise(44100 * 3, 7, 40.0), noise(44100 * 3, 8, 40.0)];
    let n = x[0].len();
    let mut bufs = vec![vec![0.0; 512]; 2];
    let mut pos = 0;
    let mut step = 0u64;
    while pos < n {
        let m = 512.min(n - pos);
        step += 1;
        // flip parameters around every block
        p.clip_mode = ClipMode::from_index((step % 3) as usize);
        p.curve = if step % 2 == 0 { Curve::A } else { Curve::B };
        p.sat = (step % 7) as f64;
        p.contour = (step % 3) as f64;
        p.ceiling_db = -((step % 37) as f64);
        p.input_db = (step % 25) as f64 - 12.0;
        p.tone = ToneMode::from_index((step % 3) as usize);
        p.clip_filter = step % 5 == 0;
        p.guard = step % 4 < 2;
        p.unity = step % 6 < 3;
        p.delta = step % 11 == 0;
        p.clip_on = step % 13 != 0;
        e.set_params(&p);
        for c in 0..2 {
            bufs[c][..m].copy_from_slice(&x[c][pos..pos + m]);
        }
        let mut refs: Vec<&mut [f64]> = bufs.iter_mut().map(|b| &mut b[..m]).collect();
        e.process(&mut refs, m);
        for b in &bufs {
            for v in &b[..m] {
                assert!(v.is_finite(), "non-finite output at block {step}");
                assert!(v.abs() < 1e3);
            }
        }
        pos += m;
    }
}

#[test]
fn process_does_not_allocate() {
    let mut e = Engine::new(48000.0, 2, 1024);
    let mut p = Params { sat: 3.0, contour: 1.5, guard: true, tone: ToneMode::B, ..Default::default() };
    e.set_params(&p);
    e.snap();
    let mut a = noise(1024, 9, 2.0);
    let mut b = noise(1024, 10, 2.0);
    for k in 0..200 {
        p.quality = Quality::from_index(k / 50);
        p.clip_mode = ClipMode::from_index(k % 3);
        e.set_params(&p);
        let mut refs: [&mut [f64]; 2] = [&mut a[..], &mut b[..]];
        ARMED.with(|a| a.set(true));
        e.process(&mut refs, 1024);
        ARMED.with(|a| a.set(false));
    }
    assert_eq!(ALLOCS.load(Ordering::SeqCst), 0, "process allocated");
}

#[test]
fn guard_holds_true_peak() {
    let fs = 48000.0;
    let p = Params { clip_mode: ClipMode::Hard, guard: true, input_db: 12.0, link: false, ..Default::default() };
    let mut e = engine(fs, p);
    // music-like spectrum: white noise through a one-pole low-pass (~3 kHz),
    // driven 12 dB into the clipper
    let w = noise(48000, 11, 1.0);
    let mut s = 0.0;
    let x: Vec<f64> = w.iter().map(|v| { s += 0.33 * (v - s); 1.2 * s }).collect();
    let y = run(&mut e, &[x.clone(), x].to_vec(), 512);
    // 16x oversampled true peak via windowed-sinc interpolation
    let h = sunbliss_core::design::lowpass(16 * 32 + 1, 0.5 / 16.0, 8.0);
    let mut tp: f64 = 0.0;
    for i in 4000..47000 {
        for ph in 0..16 {
            let mut s = 0.0;
            for k in 0..33 {
                let idx = 16 * k + ph;
                if idx < h.len() {
                    s += 16.0 * h[idx] * y[0][i + 16 - k];
                }
            }
            tp = tp.max(s.abs());
        }
    }
    let tp_db = 20.0 * tp.log10();
    assert!(tp_db < 0.0, "true peak {tp_db} dBTP");
}
