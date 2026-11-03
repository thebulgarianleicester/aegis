# Experimental process-group scoring

`crates/aegis-sprt` implements a **time-decayed, reflected log-likelihood heuristic**, not a classical Wald SPRT. Its name follows the blueprint's module naming, not a claim about statistical guarantees. This is a pure Rust library: it has no sensors, process lineage, persisted state, daemon, signals, cgroup access, automatic recovery or integration with the classifier. Do not use its outputs to contain real processes.

## Model

For each supplied group identifier, the caller provides a monotonic timestamp in nanoseconds and a vector of optional Boolean observations: `uniform`, `header_change`, `unlink_or_rename`, `directory_jump`, and `canary`. Each feature has caller-supplied probabilities `p0 = P(feature=true | benign)` and `p1 = P(feature=true | suspicious)`, strictly between zero and one. The library has **no default likelihoods or threshold**; examples in tests are illustrative only.

For an observed feature, the log-likelihood increment is `ln(p1) - ln(p0)` if true, or `ln(1-p1) - ln(1-p0)` if false. `None` means unknown and contributes nothing; it is not interpreted as false. The five contributions are added, an approximation assuming conditional independence. For group `g`, on the next event the score is:

```text
S_g = max(0, previous_score * exp(-decay_per_second * elapsed_seconds) + increment)
```

A result returns the increment, decay factor, score, peak score, observation count, and either `Continue` or `AboveThreshold`. This decision is a threshold comparison, **not** a command or a calibrated posterior probability. It is not latched; a later observation can decay it back below the threshold. A canary feature has no special unconditional action. Snapshot values do not decay in the background; decay is applied only when the next event for that group arrives.

## Boundaries and safety

- Decay, reflection at zero, repeated testing, dependent observations and estimated feature likelihoods invalidate any automatic claim of classical SPRT α/β guarantees. An α=10⁻⁴ formula alone would not calibrate this implementation. Workload-level error rates and thresholds require independent, realistic validation.
- The caller supplies group identities and is responsible for event ordering and a consistent monotonic clock. PID reuse, worker attribution and process-tree membership are not solved here. Out-of-order timestamps for a group return an error without updating it; groups remain independent.
- `max_groups` limits memory; the caller must explicitly `forget` retired groups. No implicit eviction occurs. This limit and `forget` are not a substitute for secure process-lifecycle tracking.
- Multiple signals from one filesystem event can be correlated. Counting them as independent can greatly overstate evidence. Source measurement quality, missingness and canary accessibility need explicit calibration and policy review.
- There is no built-in config file or network/control interface. Integrating a caller requires deliberate probabilities, threshold and lifecycle behavior. Tests use hand-picked values to validate the arithmetic, not to establish efficacy.

Run `cargo test -p aegis-sprt` for deterministic arithmetic, missingness, decay, ordering, group isolation/capacity and property tests. Representative benchmark corpora and held-out end-to-end tests are still required before considering any operational policy.
