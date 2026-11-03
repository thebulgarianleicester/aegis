//! Experimental time-decayed evidence scoring, not a classical SPRT.
//!
//! Scores are not calibrated probabilities or guaranteed error bounds. This
//! crate does not gather telemetry, identify process trees, or signal processes.
mod config;
mod scorer;

pub use config::{Config, Likelihood, ScoringError};
pub use scorer::{Decision, Evidence, Scorer, Snapshot, Update};
