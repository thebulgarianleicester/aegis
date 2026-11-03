use crate::model::*;
use aegis_classifier::BlockFeatures;
use aegis_control::CanaryManager;
use aegis_simulator::{
    run, RunConfig, RunSummary, SimEvent, SimulationControl, Workload,
};
use aegis_sprt::{Config, Decision, Evidence, Scorer};
use anyhow::{bail, Context, Result};
use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_OBSERVATIONS: usize = 1500;
const MAX_EVENTS: usize = 32;

struct Engine {
    status: SystemStatus,
    processes: Vec<ProcessView>,
    observations: VecDeque<ObservationView>,
    ledger: Vec<aegis_journal::TxEntry>,
    runs: Vec<RunView>,
    canaries: Vec<aegis_control::Canary>,
    alerts: VecDeque<AlertView>,
    scorer: Scorer,
    sequence: u64,
    run_counter: u64,
}

impl Engine {
    fn new() -> Result<Self> {
        let config = Config::lab_demo();
        Ok(Self {
            status: SystemStatus {
                schema_version: 2,
                mode: "isolated_lab",
                protection_active: false,
                mount: "not_mounted",
                transport: "websocket",
                active_run: None,
                cow_bytes: 0,
                threshold: config.threshold(),
                warning: "Synthetic lab only. No FUSE mount, kernel interception, or host containment is active.",
            },
            processes: Vec::new(),
            observations: VecDeque::new(),
            ledger: Vec::new(),
            runs: Vec::new(),
            canaries: Vec::new(),
            alerts: VecDeque::new(),
            scorer: Scorer::new(config)?,
            sequence: 0,
            run_counter: 0,
        })
    }

    fn telemetry(&self) -> Telemetry {
        Telemetry {
            status: SystemStatus {
                active_run: self.status.active_run.clone(),
                ..self.status.clone()
            },
            processes: self.processes.clone(),
            observations: self.observations.iter().cloned().collect(),
            ledger: self.ledger.clone(),
            runs: self.runs.clone(),
            canaries: self.canaries.clone(),
            alerts: self.alerts.iter().cloned().collect(),
        }
    }

    fn process_mut(&mut self, ragid: &str) -> Result<&mut ProcessView> {
        self.processes
            .iter_mut()
            .find(|process| process.ragid == ragid)
            .context("unknown process group")
    }

    fn run_mut(&mut self, ragid: &str) -> Result<&mut RunView> {
        self.runs
            .iter_mut()
            .find(|run| run.ragid == ragid)
            .context("unknown simulator run")
    }

    fn score(
        &mut self,
        ragid: &str,
        timestamp_ns: u64,
        evidence: Evidence,
    ) -> Result<bool> {
        let update = self.scorer.observe(ragid, timestamp_ns, evidence)?;
        let contained = update.decision == Decision::AboveThreshold;
        let newly_contained;
        let frozen;
        {
            let process = self.process_mut(ragid)?;
            process.score = update.snapshot.score;
            process.peak_score = update.snapshot.peak_score;
            process.trajectory.push(TrajectoryPoint {
                elapsed_ms: (timestamp_ns.saturating_sub(process.started_ns)) / 1_000_000,
                score: update.snapshot.score,
            });
            if process.trajectory.len() > 300 {
                let excess = process.trajectory.len() - 300;
                process.trajectory.drain(..excess);
            }
            newly_contained = contained && process.status != "FROZEN";
            if newly_contained {
                process.status = "FROZEN".to_owned();
                process.containment_ms = Some(
                    (timestamp_ns.saturating_sub(process.started_ns)) / 1_000_000,
                );
            }
            frozen = process.status == "FROZEN";
        }
        if newly_contained {
            self.alerts.push_back(AlertView {
                timestamp_ns,
                severity: "CRITICAL".to_owned(),
                ragid: ragid.to_owned(),
                message: "SPRT threshold crossed; simulated tree frozen and CoW rollback requested."
                    .to_owned(),
            });
            self.alerts.truncate_front(MAX_EVENTS);
        }
        Ok(frozen)
    }

    fn write(&mut self, event: aegis_simulator::WriteObservation) -> Result<SimulationControl> {
        let timestamp_ns = now_ns();
        let uniform = crypto_uniform(&event.features);
        let evidence = Evidence {
            uniform: Some(uniform),
            header_change: Some(event.features.header_destroyed),
            canary: event.canary.then_some(true),
            ..Evidence::default()
        };
        self.sequence = self.sequence.saturating_add(1);
        self.observations.push_back(ObservationView {
            sequence: self.sequence,
            timestamp_ns,
            ragid: event.ragid.clone(),
            path: event.path.clone(),
            operation: event.operation.clone(),
            canary: event.canary,
            crypto_uniform: uniform,
            features: event.features.clone(),
        });
        self.observations.truncate_front(MAX_OBSERVATIONS);
        {
            let process = self.process_mut(&event.ragid)?;
            process.writes = process.writes.saturating_add(1);
            process.delta_h = event.features.delta_h;
            if !process.worker_pids.contains(&event.worker_pid) {
                process.worker_pids.push(event.worker_pid);
            }
        }
        let freeze = self.score(&event.ragid, timestamp_ns, evidence)?;
        Ok(if freeze {
            SimulationControl::Freeze
        } else {
            SimulationControl::Continue
        })
    }

    fn metadata(
        &mut self,
        ragid: &str,
        operation: String,
        canary: bool,
    ) -> Result<SimulationControl> {
        let freeze = self.score(
            ragid,
            now_ns(),
            Evidence {
                unlink_or_rename: Some(true),
                canary: canary.then_some(true),
                ..Evidence::default()
            },
        )?;
        if operation == "UNLINK" {
            self.status.cow_bytes = self.status.cow_bytes.saturating_add(4096);
        }
        Ok(if freeze {
            SimulationControl::Freeze
        } else {
            SimulationControl::Continue
        })
    }

    fn finish(&mut self, summary: RunSummary) -> Result<()> {
        self.status.cow_bytes = summary.restored_bytes;
        self.ledger = summary.ledger.clone();
        let run = self.run_mut(&summary.ragid)?;
        run.status = if summary.contained {
            "CONTAINED_RECOVERED"
        } else {
            "COMPLETED_RECOVERED"
        }
        .to_owned();
        run.touched_files = summary.touched_files;
        run.modified_bytes = summary.modified_bytes;
        run.contained = summary.contained;
        run.recovered = summary.recovered;
        run.restored_operations = summary.restored_operations;
        run.restored_bytes = summary.restored_bytes;
        run.pre_tree_blake3 = Some(summary.pre_tree_blake3);
        run.post_tree_blake3 = Some(summary.post_tree_blake3);
        self.status.active_run = None;
        let process = self.process_mut(&summary.ragid)?;
        process.status = if summary.contained {
            "RECOVERED"
        } else {
            "BENIGN_EXITED"
        }
        .to_owned();
        self.alerts.push_back(AlertView {
            timestamp_ns: now_ns(),
            severity: "INFO".to_owned(),
            ragid: summary.ragid,
            message: format!(
                "Rollback verified {} operations; directory tree hash matches the pre-run snapshot.",
                summary.restored_operations
            ),
        });
        self.alerts.truncate_front(MAX_EVENTS);
        Ok(())
    }
}

#[derive(Clone)]
pub struct AppState {
    inner: Arc<Mutex<Engine>>,
    subscribers: Arc<Mutex<Vec<Sender<String>>>>,
    lab_base: Arc<PathBuf>,
}

impl AppState {
    pub fn new(lab_base: &Path) -> Result<Self> {
        fs::create_dir_all(lab_base).context("create daemon lab base")?;
        let lab_base = lab_base.canonicalize()?;
        if lab_base.parent().is_none() {
            bail!("refusing to use filesystem root as the lab base");
        }
        Ok(Self {
            inner: Arc::new(Mutex::new(Engine::new()?)),
            subscribers: Arc::new(Mutex::new(Vec::new())),
            lab_base: Arc::new(lab_base),
        })
    }

    pub fn telemetry(&self) -> Result<Telemetry> {
        self.lock().map(|engine| engine.telemetry())
    }

    pub fn subscribe(&self) -> Result<Receiver<String>> {
        let (sender, receiver) = mpsc::channel();
        sender.send(serde_json::to_string(&self.telemetry()?)?)?;
        self.subscribers
            .lock()
            .map_err(|_| anyhow::anyhow!("subscriber lock poisoned"))?
            .push(sender);
        Ok(receiver)
    }

    fn publish(&self) -> Result<()> {
        let message = serde_json::to_string(&self.telemetry()?)?;
        self.subscribers
            .lock()
            .map_err(|_| anyhow::anyhow!("subscriber lock poisoned"))?
            .retain(|subscriber| subscriber.send(message.clone()).is_ok());
        Ok(())
    }

    pub fn start_simulation(&self, workload_name: &str) -> Result<RunView> {
        let workload = Workload::from_str(workload_name)?;
        let (run_id, ragid, data_root, journal_root, root_pid) = {
            let mut engine = self.lock()?;
            if engine.status.active_run.is_some() {
                bail!("another simulator run is already active");
            }
            engine.run_counter = engine.run_counter.saturating_add(1);
            let counter = engine.run_counter;
            let run_id = format!("run-{}-{counter:04}", now_ns() / 1_000_000);
            let ragid = format!("sim-{counter:04}-{}", &run_id[run_id.len() - 4..]);
            let run_root = self.lab_base.join("runs").join(&run_id);
            let data_root = run_root.join("data");
            let journal_root = run_root.join("journal");
            fs::create_dir_all(&run_root)?;
            let root_pid = 20_000 + counter as u32;
            let run = RunView::new(
                run_id.clone(),
                ragid.clone(),
                workload.as_str().to_owned(),
            );
            engine.status.active_run = Some(run_id.clone());
            engine.runs.insert(0, run.clone());
            engine.runs.truncate(12);
            engine.processes.insert(
                0,
                ProcessView {
                    ragid: ragid.clone(),
                    root_pid,
                    worker_pids: vec![root_pid],
                    executable: "aegis-lab".to_owned(),
                    status: "ACTIVE".to_owned(),
                    writes: 0,
                    delta_h: 0.0,
                    score: 0.0,
                    peak_score: 0.0,
                    started_ns: now_ns(),
                    containment_ms: None,
                    trajectory: Vec::new(),
                },
            );
            engine.processes.truncate(12);
            (run_id, ragid, data_root, journal_root, root_pid)
        };
        self.publish()?;
        let this = self.clone();
        let thread_ragid = ragid.clone();
        thread::Builder::new()
            .name("aegis-lab-simulator".to_owned())
            .spawn(move || {
                let result = run(
                    RunConfig {
                        workload,
                        data_root,
                        journal_root,
                        ragid,
                        root_pid,
                        files: 12,
                    },
                    |event| this.handle_event(event),
                );
                if let Err(error) = result {
                    let _ = this.fail_run(&thread_ragid, format!("{error:#}"));
                }
            })?;
        self.lock()?.run_mut(&ragid).cloned()
    }

    fn handle_event(&self, event: SimEvent) -> SimulationControl {
        let result = match event {
            SimEvent::Started { ragid, .. } => self.lock().and_then(|mut engine| {
                engine.run_mut(&ragid)?.status = "RUNNING".to_owned();
                if let Ok(manager) = CanaryManager::new(&self.lab_base.join("canaries")) {
                    engine.canaries = manager.inspect().unwrap_or_default();
                }
                Ok(SimulationControl::Continue)
            }),
            SimEvent::Write(event) => self.lock().and_then(|mut engine| engine.write(event)),
            SimEvent::Metadata {
                ragid,
                operation,
                canary,
                ..
            } => self
                .lock()
                .and_then(|mut engine| engine.metadata(&ragid, operation, canary)),
            SimEvent::Finished(summary) => self
                .lock()
                .and_then(|mut engine| engine.finish(summary))
                .map(|_| SimulationControl::Continue),
        };
        let control = result.unwrap_or(SimulationControl::Freeze);
        let _ = self.publish();
        control
    }

    fn fail_run(&self, ragid: &str, error: String) -> Result<()> {
        {
            let mut engine = self.lock()?;
            engine.status.active_run = None;
            engine.run_mut(ragid)?.status = "FAILED".to_owned();
            engine.run_mut(ragid)?.error = Some(error.clone());
            engine.process_mut(ragid)?.status = "FAILED".to_owned();
            engine.alerts.push_back(AlertView {
                timestamp_ns: now_ns(),
                severity: "ERROR".to_owned(),
                ragid: ragid.to_owned(),
                message: error,
            });
        }
        self.publish()
    }

    pub fn contain(&self, ragid: &str) -> Result<ProcessView> {
        {
            let mut engine = self.lock()?;
            let process = engine.process_mut(ragid)?;
            process.status = "FROZEN".to_owned();
            process.containment_ms = Some((now_ns().saturating_sub(process.started_ns)) / 1_000_000);
            engine.alerts.push_back(AlertView {
                timestamp_ns: now_ns(),
                severity: "WARNING".to_owned(),
                ragid: ragid.to_owned(),
                message: "Manual simulated freeze requested; rollback will occur before the next workload step.".to_owned(),
            });
        }
        self.publish()?;
        self.lock()?.process_mut(ragid).cloned()
    }

    pub fn rollback(&self, ragid: &str) -> Result<RunView> {
        // Active lab runs always roll back after a freeze. Completed runs have
        // already restored and verified their pre-run tree by construction.
        let run = self.lock()?.run_mut(ragid)?.clone();
        if run.status == "RUNNING" || run.status == "STARTING" {
            self.contain(ragid)?;
        }
        Ok(run)
    }

    pub fn deploy_canaries(&self) -> Result<Vec<aegis_control::Canary>> {
        let root = self.lab_base.join("canaries");
        fs::create_dir_all(&root)?;
        let deployed = CanaryManager::new(&root)?.deploy()?;
        self.lock()?.canaries = deployed.clone();
        self.publish()?;
        Ok(deployed)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Engine>> {
        self.inner
            .lock()
            .map_err(|_| anyhow::anyhow!("engine lock poisoned"))
    }
}

fn crypto_uniform(features: &BlockFeatures) -> bool {
    features.delta_h > 1.0
        && features.entropy_post > 7.7
        && features
            .chi2_p
            .is_some_and(|p| (0.01..=0.99).contains(&p))
}

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min(u64::MAX as u128) as u64
}

/// Small extension retained here to avoid depending on a nightly collection API.
trait TruncateFront {
    fn truncate_front(&mut self, maximum: usize);
}
impl<T> TruncateFront for VecDeque<T> {
    fn truncate_front(&mut self, maximum: usize) {
        while self.len() > maximum {
            self.pop_front();
        }
    }
}

