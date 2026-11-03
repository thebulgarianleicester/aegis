use aegis_classifier::BlockFeatures;
use aegis_control::Canary;
use aegis_journal::TxEntry;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SystemStatus {
    pub schema_version: u32,
    pub mode: &'static str,
    pub protection_active: bool,
    pub mount: &'static str,
    pub transport: &'static str,
    pub active_run: Option<String>,
    pub cow_bytes: u64,
    pub threshold: f64,
    pub warning: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrajectoryPoint {
    pub elapsed_ms: u64,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessView {
    pub ragid: String,
    pub root_pid: u32,
    pub worker_pids: Vec<u32>,
    pub executable: String,
    pub status: String,
    pub writes: u64,
    pub delta_h: f64,
    pub score: f64,
    pub peak_score: f64,
    pub started_ns: u64,
    pub containment_ms: Option<u64>,
    pub trajectory: Vec<TrajectoryPoint>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ObservationView {
    pub sequence: u64,
    pub timestamp_ns: u64,
    pub ragid: String,
    pub path: String,
    pub operation: String,
    pub canary: bool,
    pub crypto_uniform: bool,
    pub features: BlockFeatures,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunView {
    pub id: String,
    pub ragid: String,
    pub workload: String,
    pub status: String,
    pub touched_files: usize,
    pub modified_bytes: u64,
    pub contained: bool,
    pub recovered: bool,
    pub restored_operations: usize,
    pub restored_bytes: u64,
    pub pre_tree_blake3: Option<String>,
    pub post_tree_blake3: Option<String>,
    pub error: Option<String>,
}

impl RunView {
    pub fn new(id: String, ragid: String, workload: String) -> Self {
        Self {
            id,
            ragid,
            workload,
            status: "STARTING".to_owned(),
            touched_files: 0,
            modified_bytes: 0,
            contained: false,
            recovered: false,
            restored_operations: 0,
            restored_bytes: 0,
            pre_tree_blake3: None,
            post_tree_blake3: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AlertView {
    pub timestamp_ns: u64,
    pub severity: String,
    pub ragid: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Telemetry {
    pub status: SystemStatus,
    pub processes: Vec<ProcessView>,
    pub observations: Vec<ObservationView>,
    pub ledger: Vec<TxEntry>,
    pub runs: Vec<RunView>,
    pub canaries: Vec<Canary>,
    pub alerts: Vec<AlertView>,
}
