use crate::config::{Config, ScoringError};
use std::collections::HashMap;

/// None = feature unavailable, Some(false) = known absent.
#[derive(Clone, Copy, Debug, Default)]
pub struct Evidence {
    pub uniform: Option<bool>,
    pub header_change: Option<bool>,
    pub unlink_or_rename: Option<bool>,
    pub directory_jump: Option<bool>,
    pub canary: Option<bool>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Continue,
    AboveThreshold,
}
#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    pub score: f64,
    pub peak_score: f64,
    pub last_timestamp_ns: u64,
    pub observations: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct Update {
    pub previous_score: f64,
    pub decay_factor: f64,
    pub log_likelihood_increment: f64,
    pub snapshot: Snapshot,
    pub decision: Decision,
}

/// Single-owner scorer. The caller is responsible for synchronization and a
/// trustworthy process-group identifier and monotonic clock across events.
pub struct Scorer {
    config: Config,
    groups: HashMap<String, Snapshot>,
}
impl Scorer {
    pub fn new(config: Config) -> Result<Self, ScoringError> {
        config.validate()?;
        Ok(Self {
            config,
            groups: HashMap::new(),
        })
    }
    pub fn observe(
        &mut self,
        group: &str,
        timestamp_ns: u64,
        evidence: Evidence,
    ) -> Result<Update, ScoringError> {
        if group.is_empty()
            || group.len() > 128
            || !group
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        {
            return Err(ScoringError::InvalidGroup);
        }
        let old = self.groups.get(group).copied();
        if let Some(state) = old {
            if timestamp_ns < state.last_timestamp_ns {
                return Err(ScoringError::TimeWentBackwards);
            }
        } else if self.groups.len() >= self.config.max_groups {
            return Err(ScoringError::Capacity);
        }

        let elapsed_seconds =
            old.map_or(0.0, |s| (timestamp_ns - s.last_timestamp_ns) as f64 / 1e9);
        let decay = (-self.config.decay_per_second * elapsed_seconds).exp();
        let features = [
            (self.config.uniform, evidence.uniform),
            (self.config.header_change, evidence.header_change),
            (self.config.unlink_or_rename, evidence.unlink_or_rename),
            (self.config.directory_jump, evidence.directory_jump),
            (self.config.canary, evidence.canary),
        ];
        let increment = features
            .into_iter()
            .filter_map(|(model, value)| value.map(|v| model.log_ratio(v)))
            .sum::<f64>();
        let previous = old.map_or(0.0, |s| s.score);
        let score = (previous * decay + increment).max(0.0);
        let snapshot = Snapshot {
            score,
            peak_score: old.map_or(score, |s| s.peak_score.max(score)),
            last_timestamp_ns: timestamp_ns,
            observations: old.map_or(1, |s| s.observations.saturating_add(1)),
        };
        self.groups.insert(group.to_owned(), snapshot);
        Ok(Update {
            previous_score: previous,
            decay_factor: decay,
            log_likelihood_increment: increment,
            snapshot,
            decision: if score >= self.config.threshold {
                Decision::AboveThreshold
            } else {
                Decision::Continue
            },
        })
    }
    /// Returns the last event state; no idle decay is applied until an event arrives.
    pub fn snapshot(&self, group: &str) -> Option<Snapshot> {
        self.groups.get(group).copied()
    }
    /// Explicit lifecycle management; groups are not automatically evicted.
    pub fn forget(&mut self, group: &str) -> bool {
        self.groups.remove(group).is_some()
    }
    pub fn group_count(&self) -> usize {
        self.groups.len()
    }
}
