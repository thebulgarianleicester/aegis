//! Safe, deterministic workload simulator for a newly-created isolated lab.
//!
//! The simulator never accepts existing user files as input. It generates its
//! own corpus, uses fixed public test keys, and restores every transaction
//! before returning. It is not malware and provides no evasion capability.
mod chacha20;

use aegis_classifier::{classify, BlockFeatures};
use aegis_control::CanaryManager;
use aegis_journal::{RecoveryReport, TxEntry, TxJournal};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    str::FromStr,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Workload {
    Compression,
    FullEncryption,
    IntermittentEncryption,
    MultiProcess,
    Canary,
}

impl Workload {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Compression => "compression",
            Self::FullEncryption => "full_encryption",
            Self::IntermittentEncryption => "intermittent_encryption",
            Self::MultiProcess => "multi_process",
            Self::Canary => "canary",
        }
    }
}

impl FromStr for Workload {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "compression" => Ok(Self::Compression),
            "full_encryption" | "full" => Ok(Self::FullEncryption),
            "intermittent_encryption" | "intermittent" => Ok(Self::IntermittentEncryption),
            "multi_process" | "multi" => Ok(Self::MultiProcess),
            "canary" => Ok(Self::Canary),
            _ => bail!("unknown workload {value}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunConfig {
    pub workload: Workload,
    pub data_root: PathBuf,
    pub journal_root: PathBuf,
    pub ragid: String,
    pub root_pid: u32,
    pub files: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulationControl {
    Continue,
    Freeze,
}

#[derive(Debug, Clone, Serialize)]
pub struct WriteObservation {
    pub ragid: String,
    pub root_pid: u32,
    pub worker_pid: u32,
    pub executable: String,
    pub path: String,
    pub directory: String,
    pub operation: String,
    pub canary: bool,
    pub features: BlockFeatures,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SimEvent {
    Started {
        ragid: String,
        workload: String,
        root_pid: u32,
        files: usize,
    },
    Write(WriteObservation),
    Metadata {
        ragid: String,
        path: String,
        operation: String,
        canary: bool,
    },
    Finished(RunSummary),
}

#[derive(Debug, Clone, Serialize)]
pub struct RunSummary {
    pub ragid: String,
    pub workload: String,
    pub touched_files: usize,
    pub modified_bytes: u64,
    pub contained: bool,
    pub recovered: bool,
    pub restored_operations: usize,
    pub restored_bytes: u64,
    pub pre_tree_blake3: String,
    pub post_tree_blake3: String,
    pub ledger: Vec<TxEntry>,
}

pub fn run<F>(config: RunConfig, mut on_event: F) -> Result<RunSummary>
where
    F: FnMut(SimEvent) -> SimulationControl,
{
    prepare_empty_lab(&config.data_root, &config.journal_root)?;
    let canaries = CanaryManager::new(&config.data_root)?;
    canaries.deploy()?;
    seed_corpus(&config.data_root, config.files)?;
    let pre_tree_blake3 = tree_hash(&config.data_root)?;
    on_event(SimEvent::Started {
        ragid: config.ragid.clone(),
        workload: config.workload.as_str().to_owned(),
        root_pid: config.root_pid,
        files: config.files,
    });

    let mut journal = TxJournal::open(
        &config.data_root,
        &config.journal_root,
        &config.ragid,
    )?;
    let mut paths: Vec<PathBuf> = fs::read_dir(&config.data_root)?
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    paths.sort();
    if config.workload == Workload::Canary {
        paths.retain(|path| canaries.is_canary(path));
    } else {
        paths.retain(|path| !canaries.is_canary(path));
    }

    let mut touched_files = 0;
    let mut modified_bytes = 0u64;
    let mut contained = false;
    for (index, path) in paths.iter().enumerate() {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("lab filename is not UTF-8")?;
        let pre = fs::read(path)?;
        let post = transform(config.workload, &pre, index)?;
        journal.write(name, &post)?;
        touched_files += 1;
        modified_bytes = modified_bytes.saturating_add(post.len() as u64);
        let features = first_changed_block(&pre, &post)?;
        let worker_pid = if config.workload == Workload::MultiProcess {
            config.root_pid + 1 + (index % 4) as u32
        } else {
            config.root_pid
        };
        let executable = match config.workload {
            Workload::Compression => "gzip-lab",
            Workload::MultiProcess => "worker-lab",
            _ => "locker-lab",
        };
        let canary = canaries.is_canary(path);
        if on_event(SimEvent::Write(WriteObservation {
            ragid: config.ragid.clone(),
            root_pid: config.root_pid,
            worker_pid,
            executable: executable.to_owned(),
            path: name.to_owned(),
            directory: ".".to_owned(),
            operation: "WRITE".to_owned(),
            canary,
            features,
        })) == SimulationControl::Freeze
        {
            contained = true;
            break;
        }

        if matches!(
            config.workload,
            Workload::FullEncryption | Workload::MultiProcess | Workload::Canary
        ) {
            let locked = format!("{name}.locked");
            journal.rename(name, &locked)?;
            if on_event(SimEvent::Metadata {
                ragid: config.ragid.clone(),
                path: locked.clone(),
                operation: "RENAME".to_owned(),
                canary,
            }) == SimulationControl::Freeze
            {
                contained = true;
                break;
            }
            journal.unlink(&locked)?;
            if on_event(SimEvent::Metadata {
                ragid: config.ragid.clone(),
                path: locked,
                operation: "UNLINK".to_owned(),
                canary,
            }) == SimulationControl::Freeze
            {
                contained = true;
                break;
            }
        }
    }

    // Both automatic containment recovery and end-of-lab cleanup use the same
    // verified inverse log. No generated ciphertext is left in the lab.
    let RecoveryReport {
        restored_operations,
        restored_bytes,
        entries,
        ..
    } = journal.rollback()?;
    let post_tree_blake3 = tree_hash(&config.data_root)?;
    let recovered = pre_tree_blake3 == post_tree_blake3;
    if !recovered {
        bail!("directory tree hash differs after simulator rollback");
    }
    let summary = RunSummary {
        ragid: config.ragid,
        workload: config.workload.as_str().to_owned(),
        touched_files,
        modified_bytes,
        contained,
        recovered,
        restored_operations,
        restored_bytes,
        pre_tree_blake3,
        post_tree_blake3,
        ledger: entries,
    };
    on_event(SimEvent::Finished(summary.clone()));
    Ok(summary)
}

fn prepare_empty_lab(data: &Path, journal: &Path) -> Result<()> {
    if data == journal || data.starts_with(journal) || journal.starts_with(data) {
        bail!("simulator data and journal paths must not overlap");
    }
    fs::create_dir_all(data).context("create simulator data directory")?;
    fs::create_dir_all(journal).context("create simulator journal directory")?;
    if fs::read_dir(data)?.next().is_some() || fs::read_dir(journal)?.next().is_some() {
        bail!("simulator requires new, empty data and journal directories");
    }
    Ok(())
}

fn seed_corpus(root: &Path, files: usize) -> Result<()> {
    if !(4..=128).contains(&files) {
        bail!("simulator file count must be between 4 and 128");
    }
    for index in 0..files {
        let mut bytes = Vec::with_capacity(16 * 4096);
        for block in 0..16 {
            bytes.extend_from_slice(
                format!(
                    "AEGISFS SYNTHETIC LAB DOCUMENT {index:03}\n\
                     Quarter: Q4 | Block: {block:02} | Public test data only.\n\
                     Revenue, expense, assets, liabilities, research notes.\n"
                )
                .repeat(48)
                .as_bytes(),
            );
        }
        bytes.resize(16 * 4096, b' ');
        fs::write(root.join(format!("report_{index:03}.txt")), bytes)?;
    }
    Ok(())
}

fn transform(workload: Workload, pre: &[u8], index: usize) -> Result<Vec<u8>> {
    match workload {
        Workload::Compression => gzip(pre),
        Workload::IntermittentEncryption => {
            let encrypted = chacha20::transform(pre, index as u32);
            let mut intermittent = pre.to_vec();
            for (block, encrypted) in intermittent
                .chunks_exact_mut(4096)
                .zip(encrypted.chunks(4096))
                .step_by(4)
            {
                block.copy_from_slice(encrypted);
            }
            Ok(intermittent)
        }
        _ => Ok(chacha20::transform(pre, index as u32)),
    }
}

fn first_changed_block(pre: &[u8], post: &[u8]) -> Result<BlockFeatures> {
    if pre.len() == post.len() {
        for (index, (before, after)) in pre
            .chunks(4096)
            .zip(post.chunks(4096))
            .enumerate()
            .filter(|(_, (before, after))| before != after)
        {
            return classify(before, after, (index * 4096) as u64)
                .map_err(|error| anyhow::anyhow!(error));
        }
    }
    // Compression changes length and framing. Compare aligned prefixes for
    // feature visualization only; this is not an inline write reconstruction.
    let length = pre.len().min(post.len()).min(4096);
    if length == 0 {
        bail!("cannot classify an empty lab transformation");
    }
    classify(&pre[..length], &post[..length], 0).map_err(|error| anyhow::anyhow!(error))
}

fn gzip(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut child = Command::new("gzip")
        .args(["-9", "-c"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("start gzip for benign compression workload")?;
    std::io::Write::write_all(
        child
            .stdin
            .as_mut()
            .context("open gzip standard input")?,
        bytes,
    )?;
    drop(child.stdin.take());
    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!("gzip failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(output.stdout)
}

fn tree_hash(root: &Path) -> Result<String> {
    let mut paths: Vec<PathBuf> = fs::read_dir(root)?
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    paths.sort();
    let mut hasher = blake3::Hasher::new();
    for path in paths {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("tree filename is not UTF-8")?;
        hasher.update(&(name.len() as u64).to_le_bytes());
        hasher.update(name.as_bytes());
        let bytes = fs::read(path)?;
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intermittent_mode_changes_only_one_block_in_four() {
        let input = b"Aegis lab text ".repeat(4096);
        let output = transform(Workload::IntermittentEncryption, &input, 0).unwrap();
        for (index, (before, after)) in input
            .chunks(4096)
            .zip(output.chunks(4096))
            .enumerate()
        {
            assert_eq!(before != after, index % 4 == 0);
        }
    }

    #[test]
    fn chacha_round_trip_and_gzip_are_deterministic() {
        let input = b"fixed public simulator input".repeat(100);
        let encrypted = chacha20::transform(&input, 7);
        assert_ne!(encrypted, input);
        assert_eq!(chacha20::transform(&encrypted, 7), input);
        let compressed = gzip(&input).unwrap();
        assert!(compressed.starts_with(b"\x1f\x8b\x08"));
    }
}
