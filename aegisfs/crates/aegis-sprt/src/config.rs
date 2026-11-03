use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum ScoringError {
    #[error("likelihood probabilities must be finite and strictly between zero and one")]
    InvalidProbability,
    #[error("decay must be finite and nonnegative, threshold must be finite and positive, and capacity must be nonzero")]
    InvalidConfig,
    #[error("group identifier must contain 1-128 ASCII alphanumeric, dot, underscore or hyphen characters")]
    InvalidGroup,
    #[error("group capacity reached")]
    Capacity,
    #[error("group timestamp moved backwards")]
    TimeWentBackwards,
}

/// Estimated probability of observing a feature under each hypothesis.
/// Values are caller-supplied; no example values should be treated as calibrated.
#[derive(Clone, Copy, Debug)]
pub struct Likelihood {
    pub benign: f64,
    pub suspicious: f64,
}
impl Likelihood {
    pub fn new(benign: f64, suspicious: f64) -> Result<Self, ScoringError> {
        if !valid_probability(benign) || !valid_probability(suspicious) {
            return Err(ScoringError::InvalidProbability);
        }
        Ok(Self { benign, suspicious })
    }
    pub(crate) fn valid(self) -> bool {
        valid_probability(self.benign) && valid_probability(self.suspicious)
    }
    pub(crate) fn log_ratio(self, observed: bool) -> f64 {
        if observed {
            self.suspicious.ln() - self.benign.ln()
        } else {
            (-self.suspicious).ln_1p() - (-self.benign).ln_1p()
        }
    }
}
fn valid_probability(p: f64) -> bool {
    p.is_finite() && p > 0.0 && p < 1.0
}

/// Input probabilities assume conditionally independent features, which may be
/// false. Missing features are omitted, not counted as absent.
#[derive(Clone, Debug)]
pub struct Config {
    pub uniform: Likelihood,
    pub header_change: Likelihood,
    pub unlink_or_rename: Likelihood,
    pub directory_jump: Likelihood,
    pub canary: Likelihood,
    /// Exponential decay constant, in inverse seconds.
    pub decay_per_second: f64,
    /// A heuristic threshold, not a false-positive guarantee.
    pub threshold: f64,
    pub max_groups: usize,
}
impl Config {
    /// Explicitly uncalibrated probabilities for the synthetic laboratory UI.
    /// They are not production detection parameters or Wald error guarantees.
    pub fn lab_demo() -> Self {
        Self {
            uniform: Likelihood {
                benign: 0.02,
                suspicious: 0.85,
            },
            header_change: Likelihood {
                benign: 0.001,
                suspicious: 0.80,
            },
            unlink_or_rename: Likelihood {
                benign: 0.02,
                suspicious: 0.65,
            },
            directory_jump: Likelihood {
                benign: 0.03,
                suspicious: 0.70,
            },
            // Decoy access is a policy trap, not ordinary statistical evidence;
            // retain enough LLR mass that unrelated negative features cannot
            // cancel single-event containment in this synthetic profile.
            canary: Likelihood {
                benign: 1e-9,
                suspicious: 0.999_999_999,
            },
            decay_per_second: 0.08,
            threshold: 9.210_340_371_976_184,
            max_groups: 1024,
        }
    }

    pub fn threshold(&self) -> f64 {
        self.threshold
    }

    pub(crate) fn validate(&self) -> Result<(), ScoringError> {
        if ![
            self.uniform,
            self.header_change,
            self.unlink_or_rename,
            self.directory_jump,
            self.canary,
        ]
        .into_iter()
        .all(Likelihood::valid)
        {
            return Err(ScoringError::InvalidProbability);
        }
        if !self.decay_per_second.is_finite()
            || self.decay_per_second < 0.0
            || !self.threshold.is_finite()
            || self.threshold <= 0.0
            || self.max_groups == 0
        {
            return Err(ScoringError::InvalidConfig);
        }
        Ok(())
    }
}
