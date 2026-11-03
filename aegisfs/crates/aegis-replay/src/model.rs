use aegis_classifier::BlockFeatures;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Probability {
    pub benign: f64,
    pub suspicious: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayConfig {
    pub uniform: Probability,
    pub header_change: Probability,
    pub unlink_or_rename: Probability,
    pub directory_jump: Probability,
    pub canary: Probability,
    pub decay_per_second: f64,
    pub threshold: f64,
    pub max_groups: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub group: String,
    pub timestamp_ns: u64,
    pub pre: PathBuf,
    pub post: PathBuf,
    pub offset: u64,
    pub length: usize,
    #[serde(default)]
    pub unlink_or_rename: Option<bool>,
    #[serde(default)]
    pub directory_jump: Option<bool>,
    #[serde(default)]
    pub canary: Option<bool>,
}
#[derive(Serialize)]
pub struct EvidenceRecord {
    pub uniform: Option<bool>,
    pub header_change: Option<bool>,
    pub unlink_or_rename: Option<bool>,
    pub directory_jump: Option<bool>,
    pub canary: Option<bool>,
}
#[derive(Serialize)]
pub struct ResultRow {
    pub group: String,
    pub timestamp_ns: u64,
    /// Seconds since the earliest event, safe for plotting without converting
    /// absolute 64-bit nanosecond timestamps to a JavaScript number.
    pub elapsed_seconds: f64,
    pub block: BlockFeatures,
    pub evidence: EvidenceRecord,
    pub increment: f64,
    pub decay: f64,
    pub score: f64,
    pub peak_score: f64,
    pub decision: &'static str,
}
#[derive(Serialize)]
pub struct Report {
    pub schema: &'static str,
    pub warning: &'static str,
    pub threshold: f64,
    pub events: Vec<ResultRow>,
}
