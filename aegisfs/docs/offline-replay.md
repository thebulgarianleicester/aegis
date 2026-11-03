# Offline classifier and scorer replay

`aegis-replay` reads before/after file extents described by JSON Lines, runs `aegis-classifier`, feeds selected features into `aegis-sprt`, and emits a JSON report with per-group trajectories. It does not watch the filesystem, perform real operations, derive process identity, signal processes, call the journal, or provide protection. Its inputs and example probabilities are **synthetic**. It reads file contents without modifying them.

## Generate and view the demonstration

From the workspace root, choose fresh paths for each run:

```sh
# Omit this first command if you already have the synthetic corpus.
python3 scripts/make_corpus.py synthetic-corpus --samples 10 --seed 2026
python3 scripts/make_replay_demo.py synthetic-corpus replay-demo
cargo run --release -p aegis-replay -- \
  --config replay-demo/config.json --events replay-demo/events.jsonl \
  --output replay-result.report.json
```

Both generators require output directories that do not already exist; `--output` creates a new report and refuses to overwrite any existing path. If `--output` is omitted, the JSON goes to stdout; shell redirection can overwrite files, so select the destination carefully. The replay reads at most 10,000 events and allows lines up to 16 KiB. Open the React console and use **Load replay JSON** in the offline score replay panel. The browser reads the selected file locally.

The existing run at `trajectory.report.json` uses 16 chunks for each of three synthetic file pairs and hand-picked, **uncalibrated** parameters. With the default generator and seed, full AES text crosses the threshold, the benign gzip replacement also crosses it, and intermittent AES text does not. This illustrates both a false alarm and a miss in this particular demonstration—not representative process-level accuracy. Snapshots include unchanged chunks; they are not a trace of actual write syscalls.

## Event and feature semantics

Each nonempty event line has `group` (synthetic caller-supplied ID), `timestamp_ns` (monotonic within each group), `pre` and `post` (paths relative to the events file), `offset`, and `length` (1–4096). Both files must contain that full extent. Optional `unlink_or_rename`, `directory_jump`, and `canary` fields are Boolean or omitted/null. They are supplied by the event author; no sensor verifies them. Example:

```json
{"group":"demo-worker","timestamp_ns":100000000,"pre":"before.bin","post":"after.bin","offset":0,"length":4096}
```

For sufficiently long blocks the replay maps a χ² **upper-tail** p-value in `[0.01, 0.99]` to `uniform=true`. This means only compatibility with a simple byte-uniform null under an asymptotic test; it does **not** prove encryption or randomness. When the test is unavailable the evidence is unknown. Header-change evidence is known only for offset zero with a recognized preimage signature; otherwise it is unknown. The model currently does not feed ΔH, KL or stride into the score. The hardcoded uniformity interval is an illustrative policy, not calibrated statistics.

Timestamps must be nondecreasing *per group*; crossing the threshold is reported but never performs any action and is not latched. The report includes `elapsed_seconds`, derived from the earliest timestamp in Rust for visualization, so a chart need not subtract potentially imprecise JavaScript representations of absolute nanosecond integers. No guarantees are made about path races or inputs changing during reading; run on stable, trusted offline snapshots.

See [scoring engine](scoring-engine.md) and [synthetic evaluation](synthetic-evaluation.md) for model and evaluation limits.
