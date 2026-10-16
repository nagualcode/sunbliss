//! The complete Sunbliss processor.
//!
//! ```text
//! x ─┬─ Tone ─ ×in ─┬─ detector ─ law ─ (align) ───────────────┐
//!    │              └─ lookahead ─ ↑L ─ pad ─ Clipper ─ Contour ─ Saturation ─ ↓L ─ clamp ─┐
//!    │                           └─ (linear path, when nothing non-linear is active) ─────┤
//!    │                                                                       Guard ─ ×unity ─ wet
//!    └─ delay(latency) ─ dry ──────────────── mix / delta ─ ×out ─ bypass ─ y
//! ```
//! `process` never allocates; everything is sized in `new`.

use crate::contour::{self, ContourStage, Detector};
use crate::converter::{ClipMode, ClipSettings, ClipStage, ClipTables, Ramp};
use crate::curves::{self, SatACoeffs, SatB, SatBCoeffs};
use crate::fir::Delay;
use crate::guard::Guard;
use crate::oversampling::{Oversampler, Quality};
use crate::tone::{self, Tone, ToneMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Curve {
    A,
    B,
}

/// Plain parameter values (targets). Units as in docs/CONTROLS.md.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub input_db: f64,
    pub output_db: f64,
    pub link: bool,
    pub link_offset_db: f64,
    pub ceiling_db: f64,
    pub clip_on: bool,
    pub clip_mode: ClipMode,
    pub clip_filter: bool,
    pub guard: bool,
    pub sat_on: bool,
    /// 0..6 (nominal dB)
    pub sat: f64,
    pub curve: Curve,
    pub unity: bool,
    pub contour_on: bool,
    /// 0..2 dB
    pub contour: f64,
    pub tone: ToneMode,
    /// 0..1
    pub wet: f64,
    /// 0..1
    pub dry: f64,
    pub mix_link: bool,
    pub wet_mute: bool,
    pub dry_mute: bool,
    pub delta: bool,
    pub bypass: bool,
    pub quality: Quality,
    /// Quality used for offline renders (None = same as `quality`).
    pub render_quality: Option<Quality>,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            input_db: 0.0,
            output_db: 0.0,
            link: true,
            link_offset_db: 0.0,
            ceiling_db: 0.0,
            clip_on: true,
            clip_mode: ClipMode::ConverterB,
            clip_filter: false,
            guard: false,
            sat_on: true,
            sat: 0.0,
            curve: Curve::A,
            unity: false,
            contour_on: true,
            contour: 0.0,
            tone: ToneMode::Flat,
            wet: 1.0,
            dry: 0.0,
            mix_link: true,
            wet_mute: false,
            dry_mute: false,
            delta: false,
            bypass: false,
            quality: Quality::High,
            render_quality: None,
        }
    }
}

impl Params {
    /// Everything off: the processor must be bit-transparent (delay only).
    pub fn neutral() -> Self {
        Params { clip_on: false, link: false, ..Default::default() }
    }
}

/// One-pole smoother on a linear value, snapping when close.
#[derive(Clone, Copy)]
struct Smooth {
    v: f64,
    a: f64,
}

impl Smooth {
    fn new(v: f64, fs: f64, tau: f64) -> Self {
        Smooth { v, a: 1.0 - (-1.0 / (tau * fs)).exp() }
    }
    #[inline]
    fn tick(&mut self, t: f64) -> f64 {
        let d = t - self.v;
        if d.abs() < 1e-12 { self.v = t } else { self.v += d * self.a }
        self.v
    }
}

#[inline]
fn db2lin(db: f64) -> f64 {
    if db == 0.0 { 1.0 } else { 10f64.powf(db / 20.0) }
}

/// Everything that depends on the oversampling factor, per channel.
struct Chain {
    l: usize,
    os: Vec<Oversampler>,
    pad: Vec<Delay>,
    clip: Vec<ClipStage>,
    contour: Vec<ContourStage>,
    /// latency of the oversampled section, base samples
    lat: usize,
    /// alignment of the contour gain, base samples
    gain_delay: usize,
}

impl Chain {
    fn new(fs: f64, factor: usize, channels: usize, max_block: usize) -> Self {
        let os: Vec<Oversampler> = (0..channels).map(|_| Oversampler::new(factor, max_block)).collect();
        let top_rate = fs * factor as f64;
        let contour: Vec<ContourStage> = (0..channels).map(|_| ContourStage::new(top_rate)).collect();
        let l = factor;
        let total = os[0].latency_top() + contour[0].delay() as f64;
        assert!((total.fract()).abs() < 1e-9, "non-integer top latency {total}");
        let total = total.round() as usize;
        let pad_n = (l - total % l) % l;
        let mut pad: Vec<Delay> = (0..channels).map(|_| Delay::new(pad_n.max(1))).collect();
        pad.iter_mut().for_each(|d| d.set(pad_n));
        let lat = (total + pad_n) / l;
        let adaa_half = if os[0].uses_adaa() { 0.5 } else { 0.0 };
        let g = (os[0].latency_up_top() + pad_n as f64 + adaa_half + contour[0].delay() as f64) / l as f64;
        Chain {
            l,
            os,
            pad,
            clip: (0..channels).map(|_| ClipStage::new(top_rate)).collect(),
            contour,
            lat,
            gain_delay: g.round() as usize,
        }
    }

    fn reset(&mut self) {
        self.os.iter_mut().for_each(|o| o.reset());
        self.pad.iter_mut().for_each(|d| d.reset());
        self.clip.iter_mut().for_each(|c| c.reset());
        self.contour.iter_mut().for_each(|c| c.reset());
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Meters {
    /// Max clip amount (dB above ceiling) at the clipper input, this block.
    pub clip_db: f64,
    /// Guard gain reduction this block (dB, <= 0).
    pub guard_db: f64,
}

pub struct Engine {
    fs: f64,
    channels: usize,
    max_block: usize,
    p: Params,
    offline: bool,
    tables: ClipTables,
    satb: SatB,
    tone: Tone,
    chains: [Chain; 3],
    active: usize,
    pending: Option<usize>,
    qfade: Ramp,
    detectors: Vec<Detector>,
    look: Vec<Delay>,
    gain_align: Vec<Delay>,
    lin_delay: Vec<Delay>,
    dry_delay: Vec<Delay>,
    guard: Guard,
    // smoothers
    s_in: Smooth,
    s_out: Smooth,
    s_ceil: Smooth,
    s_sat: Smooth,
    s_contour: Smooth,
    s_wet: Smooth,
    s_dry: Smooth,
    s_unity: Smooth,
    r_os: Ramp,
    r_wmute: Ramp,
    r_dmute: Ramp,
    r_delta: Ramp,
    r_bypass: Ramp,
    r_curve_b: Ramp,
    // per-block scratch (base rate)
    a_in: Vec<f64>,
    a_out: Vec<f64>,
    a_ceil: Vec<f64>,
    a_wet: Vec<f64>,
    a_dry: Vec<f64>,
    a_unity: Vec<f64>,
    a_contour: Vec<f64>,
    a_os: Vec<f64>,
    a_curve_b: Vec<f64>,
    a_sat_on: Vec<bool>,
    a_sat_a: Vec<SatACoeffs>,
    a_sat_b: Vec<SatBCoeffs>,
    kbuf: Vec<f64>,
    dry: Vec<Vec<f64>>,
    work: Vec<Vec<f64>>,
    lin: Vec<Vec<f64>>,
    meters: Meters,
    max_clip_db: f64,
    os_running: bool,
    prime: usize,
}

const FADE_S: f64 = 0.010;
const SMOOTH_S: f64 = 0.020;

impl Engine {
    pub fn new(fs: f64, channels: usize, max_block: usize) -> Self {
        let chains = [
            Chain::new(fs, Quality::High.factor(fs), channels, max_block),
            Chain::new(fs, Quality::Pristine.factor(fs), channels, max_block),
            Chain::new(fs, Quality::Extra.factor(fs), channels, max_block),
        ];
        let max_lat_os = chains.iter().map(|c| c.lat).max().unwrap();
        let det0 = Detector::new(fs);
        let look_n = det0.lookahead();
        let guard = Guard::new(fs, channels);
        let max_total = tone::DELAY + look_n + max_lat_os + guard.delay();
        let max_gd = chains.iter().map(|c| c.gain_delay).max().unwrap();
        let ramp = |v: f64| {
            let mut r = Ramp::new(v);
            r.set_len(FADE_S * fs);
            r
        };
        let p = Params::default();
        let mut e = Engine {
            fs,
            channels,
            max_block,
            p,
            offline: false,
            tables: ClipTables::new(),
            satb: SatB::new(),
            tone: Tone::new(fs, channels, max_block),
            chains,
            active: 0,
            pending: None,
            qfade: ramp(1.0),
            detectors: (0..channels).map(|_| Detector::new(fs)).collect(),
            look: (0..channels).map(|_| Delay::new(look_n)).collect(),
            gain_align: (0..channels).map(|_| Delay::new(max_gd)).collect(),
            lin_delay: (0..channels).map(|_| Delay::new(max_lat_os)).collect(),
            dry_delay: (0..channels).map(|_| Delay::new(max_total)).collect(),
            guard,
            s_in: Smooth::new(1.0, fs, SMOOTH_S),
            s_out: Smooth::new(1.0, fs, SMOOTH_S),
            s_ceil: Smooth::new(1.0, fs, SMOOTH_S),
            s_sat: Smooth::new(0.0, fs, SMOOTH_S),
            s_contour: Smooth::new(0.0, fs, SMOOTH_S),
            s_wet: Smooth::new(1.0, fs, SMOOTH_S),
            s_dry: Smooth::new(0.0, fs, SMOOTH_S),
            s_unity: Smooth::new(1.0, fs, SMOOTH_S),
            r_os: ramp(1.0),
            r_wmute: ramp(0.0),
            r_dmute: ramp(0.0),
            r_delta: ramp(0.0),
            r_bypass: ramp(0.0),
            r_curve_b: ramp(0.0),
            a_in: vec![0.0; max_block],
            a_out: vec![0.0; max_block],
            a_ceil: vec![0.0; max_block],
            a_wet: vec![0.0; max_block],
            a_dry: vec![0.0; max_block],
            a_unity: vec![0.0; max_block],
            a_contour: vec![0.0; max_block],
            a_os: vec![0.0; max_block],
            a_curve_b: vec![0.0; max_block],
            a_sat_on: vec![false; max_block],
            a_sat_a: vec![SatACoeffs::new(1.0); max_block],
            a_sat_b: vec![SatB::new().coeffs(0.0); max_block],
            kbuf: vec![1.0; max_block],
            dry: vec![vec![0.0; max_block]; channels],
            work: vec![vec![0.0; max_block]; channels],
            lin: vec![vec![0.0; max_block]; channels],
            meters: Meters::default(),
            max_clip_db: 0.0,
            os_running: true,
            prime: 0,
        };
        e.look.iter_mut().for_each(|d| d.set(look_n));
        e.apply_latency();
        e.set_params(&p);
        e.snap();
        e
    }

    pub fn sample_rate(&self) -> f64 {
        self.fs
    }
    pub fn channels(&self) -> usize {
        self.channels
    }
    pub fn max_block(&self) -> usize {
        self.max_block
    }
    pub fn meters(&self) -> Meters {
        self.meters
    }
    pub fn max_clip_db(&self) -> f64 {
        self.max_clip_db
    }
    pub fn reset_clip_meter(&mut self) {
        self.max_clip_db = 0.0;
    }
    pub fn oversampling(&self) -> usize {
        self.chains[self.active].l
    }

    /// Total latency in base samples (report this to the host).
    pub fn latency(&self) -> usize {
        tone::DELAY + self.look[0].delay() + self.chains[self.active].lat + self.guard.delay()
    }

    pub fn set_offline(&mut self, offline: bool) {
        self.offline = offline;
        self.pick_chain(false);
    }

    fn wanted_chain(&self) -> usize {
        let q = if self.offline { self.p.render_quality.unwrap_or(self.p.quality) } else { self.p.quality };
        match q {
            Quality::High => 0,
            Quality::Pristine => 1,
            Quality::Extra => 2,
        }
    }

    fn pick_chain(&mut self, immediate: bool) {
        let w = self.wanted_chain();
        if w == self.active {
            self.pending = None;
            return;
        }
        if immediate {
            self.active = w;
            self.pending = None;
            self.chains[w].reset();
            self.apply_latency();
        } else {
            self.pending = Some(w);
        }
    }

    fn apply_latency(&mut self) {
        let ch = &self.chains[self.active];
        let (lat_os, gd) = (ch.lat, ch.gain_delay);
        let total = self.latency();
        self.lin_delay.iter_mut().for_each(|d| d.set(lat_os));
        self.gain_align.iter_mut().for_each(|d| d.set(gd));
        self.dry_delay.iter_mut().for_each(|d| d.set(total));
    }

    pub fn set_params(&mut self, p: &Params) {
        self.p = *p;
        self.pick_chain(false);
    }

    pub fn params(&self) -> &Params {
        &self.p
    }

    /// Jump smoothers and crossfades to their targets (after prepare/reset).
    pub fn snap(&mut self) {
        let t = self.targets();
        self.s_in.v = t.g_in;
        self.s_out.v = t.g_out;
        self.s_ceil.v = t.ceil;
        self.s_sat.v = t.sat;
        self.s_contour.v = t.contour;
        self.s_wet.v = t.wet;
        self.s_dry.v = t.dry;
        self.s_unity.v = t.unity;
        self.r_os.snap(t.os);
        self.r_wmute.snap(t.wmute);
        self.r_dmute.snap(t.dmute);
        self.r_delta.snap(t.delta);
        self.r_bypass.snap(t.bypass);
        self.r_curve_b.snap(t.curve_b);
        let cs = self.clip_settings();
        for c in self.chains.iter_mut() {
            c.clip.iter_mut().for_each(|s| s.snap(&cs));
        }
        self.tone.snap(self.p.tone);
        self.pick_chain(true);
    }

    pub fn reset(&mut self) {
        for c in self.chains.iter_mut() {
            c.reset();
        }
        self.tone.reset();
        self.detectors.iter_mut().for_each(|d| d.reset());
        for v in [&mut self.look, &mut self.gain_align, &mut self.lin_delay, &mut self.dry_delay] {
            v.iter_mut().for_each(|d| d.reset());
        }
        self.guard.reset();
        self.snap();
    }

    fn clip_settings(&self) -> ClipSettings {
        ClipSettings { enabled: self.p.clip_on, mode: self.p.clip_mode, filter: self.p.clip_filter }
    }

    fn targets(&self) -> Targets {
        let p = &self.p;
        let sat = if p.sat_on { p.sat.clamp(0.0, 6.0) } else { 0.0 };
        let contour = if p.contour_on { p.contour.clamp(0.0, 2.0) } else { 0.0 };
        let out_db = if p.link { -p.input_db + p.link_offset_db } else { p.output_db };
        let ceil = if p.clip_on { db2lin(p.ceiling_db.clamp(-36.0, 0.0)) } else { 1.0 };
        let unity = if p.unity && sat > 0.0 {
            match p.curve {
                Curve::B => db2lin(-sat),
                Curve::A => db2lin(-curves::knob_to_gain_a_db(sat)),
            }
        } else {
            1.0
        };
        let wet = p.wet.clamp(0.0, 1.0);
        let dry = if p.mix_link { 1.0 - wet } else { p.dry.clamp(0.0, 1.0) };
        Targets {
            g_in: db2lin(p.input_db),
            g_out: db2lin(out_db),
            ceil,
            sat,
            contour,
            wet,
            dry,
            unity,
            os: if p.clip_on || sat > 0.0 || contour > 0.0 { 1.0 } else { 0.0 },
            wmute: if p.wet_mute { 1.0 } else { 0.0 },
            dmute: if p.dry_mute { 1.0 } else { 0.0 },
            delta: if p.delta { 1.0 } else { 0.0 },
            bypass: if p.bypass { 1.0 } else { 0.0 },
            curve_b: if p.curve == Curve::B { 1.0 } else { 0.0 },
        }
    }

    /// Process `n` samples of every channel in place. `n <= max_block`.
    pub fn process(&mut self, chans: &mut [&mut [f64]], n: usize) {
        assert!(n <= self.max_block && chans.len() == self.channels);
        let t = self.targets();
        let mut os_any = false;
        if t.os > 0.0 && !self.os_running {
            // waking the oversampled path: clear stale history and let it fill
            // before the crossfade starts
            self.chains[self.active].reset();
            self.prime = self.chains[self.active].lat + 1;
        }
        // ---- per-sample parameter trajectories (shared by channels)
        for i in 0..n {
            self.a_in[i] = self.s_in.tick(t.g_in);
            self.a_out[i] = self.s_out.tick(t.g_out);
            self.a_ceil[i] = self.s_ceil.tick(t.ceil);
            self.a_wet[i] = self.s_wet.tick(t.wet);
            self.a_dry[i] = self.s_dry.tick(t.dry);
            self.a_unity[i] = self.s_unity.tick(t.unity);
            self.a_contour[i] = self.s_contour.tick(t.contour);
            if self.prime > 0 {
                self.prime -= 1;
                self.a_os[i] = 0.0;
                os_any = true;
            } else {
                self.a_os[i] = self.r_os.tick(t.os);
                os_any |= self.a_os[i] > 0.0;
            }
            let cb = self.r_curve_b.tick(t.curve_b);
            self.a_curve_b[i] = cb;
            let sat = self.s_sat.tick(t.sat);
            self.a_sat_on[i] = sat > 0.0;
            if sat > 0.0 {
                self.a_sat_a[i] = SatACoeffs::new(db2lin(curves::knob_to_gain_a_db(sat)));
                self.a_sat_b[i] = self.satb.coeffs(sat);
            }
        }
        self.os_running = os_any;
        let cs = self.clip_settings();
        let ch_active = self.active;

        // ---- tone (base rate, all channels), then input trim; keep dry copy
        for (ch, buf) in chans.iter_mut().enumerate() {
            let d = &mut self.dry[ch];
            for i in 0..n {
                d[i] = self.dry_delay[ch].tick(buf[i]);
            }
        }
        self.tone.process_all(chans, n, self.p.tone);

        let mut clip_max: f64 = 0.0;
        for ch in 0..self.channels {
            let w = &mut self.work[ch];
            let buf = &chans[ch];
            for i in 0..n {
                let x = buf[i] * self.a_in[i];
                // detector (normalised to ceiling, clamped to the clip level)
                let r = x.abs() / self.a_ceil[i];
                clip_max = clip_max.max(r);
                let env = self.detectors[ch].tick(r.min(1.0));
                let k = contour::law(env, self.a_contour[i]);
                self.kbuf[i] = self.gain_align[ch].tick(k);
                w[i] = self.look[ch].tick(x);
                self.lin[ch][i] = self.lin_delay[ch].tick(w[i]);
            }
            if os_any {
                let chain = &mut self.chains[ch_active];
                let l = chain.l;
                let adaa = chain.os[ch].uses_adaa();
                let top = chain.os[ch].up(&w[..n]);
                for v in top.iter_mut() {
                    *v = chain.pad[ch].tick(*v);
                }
                chain.clip[ch].process(&self.tables, top, &self.a_ceil[..n], l, &cs, adaa);
                chain.contour[ch].process(top, &self.kbuf[..n], l);
                // saturation (Curve A / B crossfade), normalised to the ceiling
                for (j, v) in top.iter_mut().enumerate() {
                    let i = j / l;
                    if !self.a_sat_on[i] {
                        continue;
                    }
                    let c = self.a_ceil[i];
                    let u = *v / c;
                    let wb = self.a_curve_b[i];
                    let y = if wb == 0.0 {
                        self.a_sat_a[i].eval(u)
                    } else if wb == 1.0 {
                        self.a_sat_b[i].eval(u)
                    } else {
                        self.a_sat_a[i].eval(u) * (1.0 - wb) + self.a_sat_b[i].eval(u) * wb
                    };
                    *v = y * c;
                }
                chain.os[ch].down(&mut w[..n]);
                for i in 0..n {
                    // sample clamp (reference behaviour). With the guard on it is
                    // skipped: clamping re-creates energy at Nyquist and with it
                    // the inter-sample peaks the guard is there to remove.
                    if self.p.clip_on && !self.p.guard {
                        let c = self.a_ceil[i];
                        w[i] = w[i].clamp(-c, c);
                    }
                    let a = self.a_os[i];
                    if a < 1.0 {
                        w[i] = self.lin[ch][i] + a * (w[i] - self.lin[ch][i]);
                    }
                }
            } else {
                w[..n].copy_from_slice(&self.lin[ch][..n]);
            }
        }
        // ---- guard (linked), unity, mix, output, bypass
        {
            let mut refs: [&mut [f64]; crate::guard::MAX_CH] = Default::default();
            let mut it = self.work.iter_mut();
            for r in refs.iter_mut().take(self.channels) {
                *r = &mut it.next().unwrap()[..];
            }
            self.guard.process(&mut refs[..self.channels], n, &self.a_ceil[..n], self.p.guard);
        }
        for i in 0..n {
            let wm = self.r_wmute.tick(t.wmute);
            let dm = self.r_dmute.tick(t.dmute);
            let de = self.r_delta.tick(t.delta);
            let by = self.r_bypass.tick(t.bypass);
            let qf = self.qfade.tick(if self.pending.is_some() { 0.0 } else { 1.0 });
            for ch in 0..self.channels {
                let wet = self.work[ch][i] * self.a_unity[i];
                let dry = self.dry[ch][i];
                let mut o = wet * self.a_wet[i] * (1.0 - wm) + dry * self.a_dry[i] * (1.0 - dm);
                if de > 0.0 {
                    let delta = wet - dry * self.a_in[i];
                    o += de * (delta - o);
                }
                o *= self.a_out[i];
                if by > 0.0 {
                    o += by * (dry - o);
                }
                if qf < 1.0 {
                    o *= qf;
                }
                chans[ch][i] = o;
            }
        }
        // quality switch completes once fully faded out
        if let Some(w) = self.pending {
            if self.qfade.v == 0.0 {
                self.active = w;
                self.pending = None;
                self.chains[w].reset();
                let cs = self.clip_settings();
                self.chains[w].clip.iter_mut().for_each(|s| s.snap(&cs));
                self.apply_latency();
            }
        }
        let clip_db = if clip_max > 1.0 && self.p.clip_on { 20.0 * clip_max.log10() } else { 0.0 };
        self.max_clip_db = self.max_clip_db.max(clip_db);
        self.meters = Meters { clip_db, guard_db: self.guard.last_gr_db };
    }
}

struct Targets {
    g_in: f64,
    g_out: f64,
    ceil: f64,
    sat: f64,
    contour: f64,
    wet: f64,
    dry: f64,
    unity: f64,
    os: f64,
    wmute: f64,
    dmute: f64,
    delta: f64,
    bypass: f64,
    curve_b: f64,
}
