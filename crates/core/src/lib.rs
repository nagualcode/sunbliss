//! Sunbliss DSP core.
//!
//! Pure f64 processing, no allocation, locks or I/O in `Engine::process`.
//! See docs/CHAIN.md for the signal flow and docs/CONTROLS.md for the
//! parameter semantics.

pub mod contour;
pub mod converter;
pub mod curves;
pub mod design;
pub mod engine;
pub mod fir;
pub mod guard;
pub mod oversampling;
pub mod tone;
mod tone_tables;

pub use converter::ClipMode;
pub use engine::{Curve, Engine, Meters, Params};
pub use oversampling::Quality;
pub use tone::ToneMode;
