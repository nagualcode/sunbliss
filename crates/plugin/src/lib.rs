//! Sunbliss plugin: truce wrapper around `sunbliss-core`.

use std::sync::{Arc, RwLock};

use truce::prelude64::*;

use sunbliss_core::{ClipMode, Curve, Engine, Quality, ToneMode};

mod editor;
mod loudness;

use loudness::ShortTerm;

// ------------------------------------------------------------------ enums
// Names and order follow the reference plugin so settings can be copied
// across one to one (personal study build).

#[derive(ParamEnum)]
pub enum ClipperModeP {
    #[name = "Classic"]
    Classic,
    #[name = "Modern"]
    Modern,
    #[name = "Hard"]
    Hard,
}

#[derive(ParamEnum)]
pub enum GoldModeP {
    #[name = "Classic"]
    Classic,
    #[name = "Modern"]
    Modern,
}

#[derive(ParamEnum)]
pub enum BoxToneP {
    #[name = "Flat"]
    Flat,
    #[name = "Classic"]
    Classic,
    #[name = "Modern"]
    Modern,
}

#[derive(ParamEnum)]
pub enum OversamplingP {
    #[name = "High"]
    High,
    #[name = "Pristine"]
    Pristine,
    #[name = "Extra Pristine"]
    ExtraPristine,
}

#[derive(ParamEnum)]
pub enum OversamplingTypeP {
    #[name = "Linear Phase"]
    Linear,
    #[name = "Minimum Phase"]
    Minimum,
}

#[derive(ParamEnum)]
pub enum OfflineOversamplingP {
    #[name = "Same as Realtime"]
    Same,
    #[name = "High"]
    High,
    #[name = "Pristine"]
    Pristine,
    #[name = "Extra Pristine"]
    ExtraPristine,
}

// Converter section: features of the original hardware (AD122-96 MkIII
// manual) with the manual's own option names.

#[derive(ParamEnum)]
pub enum WordLengthP {
    #[name = "Float"]
    Float,
    #[name = "24 bit"]
    W24,
    #[name = "22 bit"]
    W22,
    #[name = "20 bit"]
    W20,
    #[name = "18 bit"]
    W18,
    #[name = "16 bit"]
    W16,
}

#[derive(ParamEnum)]
pub enum DitherP {
    #[name = "Off"]
    Off,
    #[name = "HPDF"]
    Hpdf,
    #[name = "Flat"]
    Flat,
}

#[derive(ParamEnum)]
pub enum NoiseShapingP {
    #[name = "Off"]
    Off,
    #[name = "NS1"]
    Ns1,
    #[name = "NS2"]
    Ns2,
    #[name = "NS3"]
    Ns3,
    #[name = "NS4"]
    Ns4,
}

/// A/B comparison state, saved with the session.
/// `active`: 0 = A and B linked (edits go to both), 1 = A, 2 = B.
/// Snapshots hold normalized values in `param_infos()` order.
#[derive(State, Default, Clone)]
pub struct AbState {
    pub active: u8,
    pub a: Vec<f64>,
    pub b: Vec<f64>,
}

// ------------------------------------------------------------------ params
// Smoothing is done inside the engine (per sample, on linear gains), so the
// host-facing params are read as plain targets once per block.

#[derive(Params)]
pub struct SunblissParams {
    #[param(name = "Input Trim", range = "linear(-12, 12)", default = 0, unit = "dB", smooth = "none")]
    pub input_trim: FloatParam,
    #[param(name = "Output Trim", range = "linear(-12, 12)", default = 0, unit = "dB", smooth = "none")]
    pub output_trim: FloatParam,
    #[param(name = "Trim Link", default = true)]
    pub trim_link: BoolParam,
    #[param(name = "Trim Link Offset", range = "linear(-12, 12)", default = 0, unit = "dB", smooth = "none")]
    pub trim_link_offset: FloatParam,

    #[param(name = "Clip Ceiling", range = "linear(-36, 0)", default = 0, unit = "dB", smooth = "none")]
    pub clip_ceiling: FloatParam,
    #[param(name = "Clipper", default = true)]
    pub clipper: BoolParam,
    #[param(name = "Clipper Mode", default = 0)]
    pub clipper_mode: EnumParam<ClipperModeP>,
    #[param(name = "Clip Filter", default = false)]
    pub clip_filter: BoolParam,
    #[param(name = "Clip Guard", default = false)]
    pub clip_guard: BoolParam,

    #[param(name = "Gold On", default = true)]
    pub gold_on: BoolParam,
    #[param(name = "Gold", range = "linear(0, 6)", default = 0, unit = "dB", smooth = "none")]
    pub gold: FloatParam,
    #[param(name = "Gold Mode", default = 1)]
    pub gold_mode: EnumParam<GoldModeP>,
    #[param(name = "Unity Gain", default = false)]
    pub unity_gain: BoolParam,

    #[param(name = "Alchemy On", default = true)]
    pub alchemy_on: BoolParam,
    #[param(name = "Alchemy", range = "linear(0, 2)", default = 0, unit = "dB", smooth = "none")]
    pub alchemy: FloatParam,

    #[param(name = "Box Tone", default = 0)]
    pub box_tone: EnumParam<BoxToneP>,

    #[param(name = "Wet", range = "linear(0, 1)", default = 1, unit = "%", smooth = "none")]
    pub wet: FloatParam,
    #[param(name = "Dry", range = "linear(0, 1)", default = 0, unit = "%", smooth = "none")]
    pub dry: FloatParam,
    #[param(name = "Dry Wet Link", default = true)]
    pub dry_wet_link: BoolParam,
    #[param(name = "Wet Mute", default = false)]
    pub wet_mute: BoolParam,
    #[param(name = "Dry Mute", default = false)]
    pub dry_mute: BoolParam,
    #[param(name = "Delta", default = false)]
    pub delta: BoolParam,
    #[param(name = "Bypass", default = false, flags = "automatable | bypass")]
    pub bypass: BoolParam,

    #[param(name = "Oversampling", default = 0)]
    pub oversampling: EnumParam<OversamplingP>,
    #[param(name = "Oversampling Type", default = 0)]
    pub oversampling_type: EnumParam<OversamplingTypeP>,
    #[param(name = "Offline Oversampling", default = 0)]
    pub offline_oversampling: EnumParam<OfflineOversamplingP>,

    #[param(name = "Polarity Invert", default = false)]
    pub polarity_invert: BoolParam,
    #[param(name = "DC Removal", default = false)]
    pub dc_removal: BoolParam,
    #[param(name = "Word Length", default = 0)]
    pub word_length: EnumParam<WordLengthP>,
    #[param(name = "Dither", default = 1)]
    pub dither: EnumParam<DitherP>,
    #[param(name = "Noise Shaping", default = 0)]
    pub noise_shaping: EnumParam<NoiseShapingP>,
    #[param(name = "Data Limit", default = false)]
    pub data_limit: BoolParam,

    // meters (read by the editor)
    #[meter]
    pub m_in_l: MeterSlot,
    #[meter]
    pub m_in_r: MeterSlot,
    #[meter]
    pub m_out_l: MeterSlot,
    #[meter]
    pub m_out_r: MeterSlot,
    /// dB over the ceiling at the clipper input this block
    #[meter]
    pub m_clip: MeterSlot,
    /// Clip Clock: held maximum (dB) and its transport time (s)
    #[meter]
    pub m_clip_max: MeterSlot,
    #[meter]
    pub m_clip_pos: MeterSlot,
    #[meter]
    pub m_guard: MeterSlot,
    #[meter]
    pub m_lufs_in: MeterSlot,
    #[meter]
    pub m_lufs_out: MeterSlot,

    #[persist = "ab"]
    pub ab: RwLock<AbState>,
    /// Bumped by the editor to reset the Clip Clock.
    #[persist = "clip_reset"]
    pub clip_reset: AtomicCell<u32>,
}

use SunblissParamsParamId as P;

impl SunblissParams {
    fn engine_params(&self) -> sunbliss_core::Params {
        sunbliss_core::Params {
            input_db: self.input_trim.value(),
            output_db: self.output_trim.value(),
            link: self.trim_link.value(),
            link_offset_db: self.trim_link_offset.value(),
            ceiling_db: self.clip_ceiling.value(),
            clip_on: self.clipper.value(),
            clip_mode: ClipMode::from_index(self.clipper_mode.index() as usize),
            clip_filter: self.clip_filter.value(),
            guard: self.clip_guard.value(),
            sat_on: self.gold_on.value(),
            sat: self.gold.value(),
            curve: if self.gold_mode.index() == 0 { Curve::Classic } else { Curve::Modern },
            unity: self.unity_gain.value(),
            contour_on: self.alchemy_on.value(),
            contour: self.alchemy.value(),
            tone: ToneMode::from_index(self.box_tone.index() as usize),
            wet: self.wet.value(),
            dry: self.dry.value(),
            mix_link: self.dry_wet_link.value(),
            wet_mute: self.wet_mute.value(),
            dry_mute: self.dry_mute.value(),
            delta: self.delta.value(),
            bypass: self.bypass.value(),
            quality: Quality::from_index(self.oversampling.index() as usize),
            render_quality: match self.offline_oversampling.index() {
                0 => None,
                i => Some(Quality::from_index(i as usize - 1)),
            },
            min_phase: self.oversampling_type.index() == 1,
            converter: sunbliss_core::ConverterSettings {
                polarity_invert: self.polarity_invert.value(),
                dc_removal: self.dc_removal.value(),
                word_length: [None, Some(24), Some(22), Some(20), Some(18), Some(16)][self.word_length.index() as usize],
                dither: match self.dither.index() {
                    1 => Some(sunbliss_core::Dither::HighPass),
                    2 => Some(sunbliss_core::Dither::Flat),
                    _ => None,
                },
                noise_shaping: self.noise_shaping.index() as u8,
                data_limit: self.data_limit.value(),
            },
        }
    }
}

// ------------------------------------------------------------------ DSP state

const CHANNELS: usize = 2;
const MAX_BLOCK: usize = 2048;

#[derive(Default)]
pub struct SunblissDsp {
    engine: Option<Engine>,
    scratch: Vec<Vec<f64>>,
    lufs_in: Option<ShortTerm>,
    lufs_out: Option<ShortTerm>,
    clip_max: f64,
    clip_pos: f64,
    reset_seen: u32,
}

pub struct Sunbliss;

impl PluginLogic for Sunbliss {
    type Params = SunblissParams;
    type DspState = SunblissDsp;

    fn reset(state: &mut SunblissDsp, params: &SunblissParams, config: &AudioConfig) {
        let block = config.max_block_size.clamp(32, MAX_BLOCK);
        let mut e = Engine::new(config.sample_rate, CHANNELS, block);
        e.set_params(&params.engine_params());
        e.set_offline(config.process_mode.is_offline());
        e.snap();
        state.engine = Some(e);
        state.scratch = vec![vec![0.0; block]; CHANNELS];
        state.lufs_in = Some(ShortTerm::new(config.sample_rate));
        state.lufs_out = Some(ShortTerm::new(config.sample_rate));
        state.clip_max = 0.0;
        state.clip_pos = 0.0;
        state.reset_seen = params.clip_reset.load();
    }

    fn process(
        state: &mut SunblissDsp,
        params: &SunblissParams,
        buffer: &mut AudioBuffer,
        _events: &EventList,
        context: &mut ProcessContext,
    ) -> ProcessStatus {
        let Some(engine) = state.engine.as_mut() else {
            return ProcessStatus::Normal;
        };
        engine.set_params(&params.engine_params());
        engine.set_offline(context.process_mode.is_offline());

        let r = params.clip_reset.load();
        if r != state.reset_seen {
            state.reset_seen = r;
            state.clip_max = 0.0;
            state.clip_pos = 0.0;
        }

        let n_total = buffer.num_samples();
        let n_in = buffer.num_input_channels();
        let n_out = buffer.num_output_channels().min(CHANNELS);
        let block = engine.max_block();
        let (mut pk_in, mut pk_out) = ([0.0f64; 2], [0.0f64; 2]);
        let lufs_in = state.lufs_in.as_mut().unwrap();
        let lufs_out = state.lufs_out.as_mut().unwrap();
        let mut off = 0;
        while off < n_total {
            let n = block.min(n_total - off);
            // gather (mono input feeds both engine channels)
            for c in 0..CHANNELS {
                let src = if n_in == 0 { None } else { Some(buffer.input(c.min(n_in - 1))) };
                let dst = &mut state.scratch[c][..n];
                match src {
                    Some(s) => dst.copy_from_slice(&s[off..off + n]),
                    None => dst.fill(0.0),
                }
            }
            for i in 0..n {
                let (l, r) = (state.scratch[0][i], state.scratch[1][i]);
                pk_in[0] = pk_in[0].max(l.abs());
                pk_in[1] = pk_in[1].max(r.abs());
                lufs_in.push(l, r);
            }
            {
                let (a, b) = state.scratch.split_at_mut(1);
                let mut refs: [&mut [f64]; 2] = [&mut a[0][..n], &mut b[0][..n]];
                engine.process(&mut refs, n);
            }
            for i in 0..n {
                let (l, r) = (state.scratch[0][i], state.scratch[1][i]);
                pk_out[0] = pk_out[0].max(l.abs());
                pk_out[1] = pk_out[1].max(r.abs());
                lufs_out.push(l, r);
            }
            for c in 0..n_out {
                buffer.output(c)[off..off + n].copy_from_slice(&state.scratch[c][..n]);
            }
            let m = engine.meters();
            if m.clip_db > state.clip_max {
                state.clip_max = m.clip_db;
                let t = context.transport;
                state.clip_pos = if t.playing { t.position_seconds + off as f64 / context.sample_rate } else { -1.0 };
            }
            off += n;
        }
        let m = engine.meters();
        context.set_meter(P::MInL, pk_in[0] as f32);
        context.set_meter(P::MInR, pk_in[1] as f32);
        context.set_meter(P::MOutL, pk_out[0] as f32);
        context.set_meter(P::MOutR, pk_out[1] as f32);
        context.set_meter(P::MClip, m.clip_db as f32);
        context.set_meter(P::MClipMax, state.clip_max as f32);
        context.set_meter(P::MClipPos, state.clip_pos as f32);
        context.set_meter(P::MGuard, m.guard_db as f32);
        context.set_meter(P::MLufsIn, lufs_in.lufs().max(-99.0) as f32);
        context.set_meter(P::MLufsOut, lufs_out.lufs().max(-99.0) as f32);
        ProcessStatus::Normal
    }

    fn latency(state: &SunblissDsp) -> u32 {
        state.engine.as_ref().map_or(0, |e| e.latency() as u32)
    }

    fn editor(params: Arc<SunblissParams>) -> Box<dyn Editor> {
        editor::create(params)
    }
}

truce::plugin! {
    logic: Sunbliss,
    params: SunblissParams,
}

truce::enable_rt_paranoid!();

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use truce_test::{InputSource, assertions, driver};

    #[test]
    fn info_is_valid() {
        truce_test::assert_valid_info::<Plugin>();
    }

    #[test]
    fn renders_and_is_finite() {
        let result = driver!(Plugin).duration(Duration::from_millis(200)).input(InputSource::Constant(0.5)).run();
        assertions::assert_no_nans(&result);
    }

    #[test]
    fn state_round_trips() {
        truce_test::assert_state_round_trip::<Plugin>();
    }

    #[test]
    fn has_editor() {
        truce_test::assert_has_editor::<Plugin>();
    }
}
