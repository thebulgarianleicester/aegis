use crate::model::{Event, EvidenceRecord, Probability, ReplayConfig, Report, ResultRow};
use aegis_classifier::classify;
use aegis_sprt::{Config, Decision, Evidence, Likelihood, Scorer};
use anyhow::{bail, Context, Result};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

fn model(p: Probability) -> Result<Likelihood> {
    Ok(Likelihood::new(p.benign, p.suspicious)?)
}
fn block(path: &Path, offset: u64, length: usize) -> Result<Vec<u8>> {
    let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    if !file.metadata()?.is_file() {
        bail!("input must be a regular file");
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut data = vec![0; length];
    file.read_exact(&mut data).with_context(|| {
        format!(
            "read {} bytes at offset {} from {}",
            length,
            offset,
            path.display()
        )
    })?;
    Ok(data)
}
pub fn run(c: ReplayConfig, events: &[Event], base: &Path) -> Result<Report> {
    let config = Config {
        uniform: model(c.uniform)?,
        header_change: model(c.header_change)?,
        unlink_or_rename: model(c.unlink_or_rename)?,
        directory_jump: model(c.directory_jump)?,
        canary: model(c.canary)?,
        decay_per_second: c.decay_per_second,
        threshold: c.threshold,
        max_groups: c.max_groups,
    };
    let threshold = config.threshold;
    let mut scorer = Scorer::new(config)?;
    let mut results = Vec::with_capacity(events.len());
    let earliest = events.iter().map(|e| e.timestamp_ns).min().unwrap_or(0);
    for (index, event) in events.iter().enumerate() {
        if event.length == 0
            || event.length > 4096
            || event.offset.checked_add(event.length as u64).is_none()
        {
            bail!("event {} has invalid extent", index + 1);
        }
        let pre = block(&base.join(&event.pre), event.offset, event.length)?;
        let post = block(&base.join(&event.post), event.offset, event.length)?;
        let f = classify(&pre, &post, event.offset).map_err(anyhow::Error::msg)?;
        // Compatibility with a uniform-byte null is NOT proof of encryption.
        let uniform = f.chi2_p.map(|p| (0.01..=0.99).contains(&p));
        let header_change = if event.offset == 0 && f.header_before.is_some() {
            Some(f.header_destroyed)
        } else {
            None
        };
        let evidence = Evidence {
            uniform,
            header_change,
            unlink_or_rename: event.unlink_or_rename,
            directory_jump: event.directory_jump,
            canary: event.canary,
        };
        let update = scorer
            .observe(&event.group, event.timestamp_ns, evidence)
            .with_context(|| format!("score event {}", index + 1))?;
        results.push(ResultRow {
            group: event.group.clone(),
            timestamp_ns: event.timestamp_ns,
            elapsed_seconds: (event.timestamp_ns - earliest) as f64 / 1e9,
            block: f,
            evidence: EvidenceRecord {
                uniform,
                header_change,
                unlink_or_rename: event.unlink_or_rename,
                directory_jump: event.directory_jump,
                canary: event.canary,
            },
            increment: update.log_likelihood_increment,
            decay: update.decay_factor,
            score: update.snapshot.score,
            peak_score: update.snapshot.peak_score,
            decision: match update.decision {
                Decision::Continue => "continue",
                Decision::AboveThreshold => "above_threshold",
            },
        });
    }
    Ok(Report {
        schema: "aegisfs-replay-v1",
        warning: "Offline, uncalibrated replay; no monitoring, containment or protection.",
        threshold,
        events: results,
    })
}
