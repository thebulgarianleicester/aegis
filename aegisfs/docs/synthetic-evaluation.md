# Synthetic classifier evaluation

This is a reproducible engineering check, **not** evidence of production readiness or a statistically meaningful ransomware-detection rate. See the project README for commands. Run `python3 scripts/summarize_report.py synthetic.report.json` for every workload/method count.

## Dataset and ground truth

The checked run used `--samples 10 --seed 2026`, producing 70 before/after pairs and 1,140 blocks. All source bytes are generated in memory. OpenSSL applies AES-256-CTR or ChaCha20 with deterministic public test keys to those bytes; it does not read files. The intermittent case replaces 4 of 16 aligned blocks (0, 4, 8, 12). `positive_blocks` marks those blocks positive; the remaining blocks are negative for **block encryption**, not benign processes. Synthetic gzip samples are generated from pseudorandom bytes, and are zero-padded if needed to give paired files equal size. The final short block is included, but its χ² p-value is absent. The PDF-like files are synthetic header-prefixed text, not well-formed PDF documents.

Cases are correlated across blocks and share generated data, and the dataset is too small and artificial for confidence intervals, representative FPR or any claimed comparison with a deployed detector. There is no train/test split and no calibration or hyperparameter sweep. The three fixed rules are not scored at a common operating point. These results cannot be interpreted as process-level rates, workload-level rates, files-before-containment, or ransomware efficacy. The generated random-rewrite workload represents a deliberately challenging benign overwrite, not a measured frequency in real life.

## Observed results on this corpus

- In `benign_random_rewrite`, the demo heuristic flagged **157 / 160** negative blocks. Static entropy flagged 160 / 160. This illustrates that high entropy after an overwrite can be benign.
- In `aes_full_gzip`, the demo heuristic flagged **10 / 170** positive blocks, while the delta-entropy baseline flagged **0 / 170**. Header destruction accounts for the demo heuristic's first-block signals; most already-high-entropy encrypted blocks have little entropy increase. The short final block is counted.
- In `benign_gzip_replacement`, static entropy flagged **160 / 170** blocks, while the demo heuristic flagged 0 / 170. The synthetic gzip stream is largely incompressible data, and is not representative of all compressed files.
- In `aes_intermittent_text`, the demo heuristic flagged **40 / 40** labeled positive blocks and 0 / 120 unchanged blocks. This is an easy aligned synthetic case, not proof of interception or partial-encryption detection in real workloads.

A genuine trial requires independently collected realistic before/after snapshots and workload telemetry, clear authorization, stratified process/workload-level labels, separate calibration and held-out sources, realistic temporal rules, confidence intervals across independent runs, and threat-model-specific adversarial validation. The current demo heuristic should not trigger containment.
