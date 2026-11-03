# AegisFS

AegisFS is a Linux ransomware-defense **research prototype** built incrementally from the Project A blueprint. This revision adds a working, isolated vertical slice: differential byte features, grouped write-ahead CoW recovery, sequential process-tree scoring, valid document canaries, safe synthetic workloads, a recovery CLI, and a live React console.

**It is not a filesystem protection product.** No protected FUSE mount, eBPF LSM sensor, kernel interception, or automatic host-process containment is active. The web simulator only transforms files that it generated inside new, empty lab directories and always invokes verified cleanup rollback. Do not run malware against this prototype or use it as a backup system.

## Implemented milestones

| Milestone | Current status |
|---|---|
| A1 — differential features and offline benchmark | Existing Shannon/ΔH, χ², KL, serial-correlation, header and stride primitives; synthetic offline comparison. Representative corpus validation and performance work remain. |
| A2 — durable CoW and rollback | **Grouped whole-file write-ahead transactions** for write/create, shrinking truncate, rename (including replacement), and unlink. BLAKE3 content-addressed pre-images, SQLite WAL metadata, capacity fail-closed behavior, reverse replay, conflict detection, and recovery tests. Not a FUSE or 4 KiB extent implementation. |
| A3 — scoring, lineage, containment | Existing time-decayed evidence scorer; PID + start-time `/proc` lineage primitive; explicit-scope, dry-run-by-default SIGSTOP/SIGKILL controller. The UI uses synthetic workers under a shared RAGID and never signals host processes. |
| A4 — eBPF and LSM self-protection | Not implemented. `/proc` telemetry is not a synchronous security boundary. |
| A5 — API and live console | Working lab REST + WebSocket daemon and React dashboard, process inspector, recovery ledger, canary matrix, and simulator. The small standard-library HTTP server replaces Axum for this dependency-light prototype; no production TLS/JWT boundary is included. |
| A6 — systems/scientific evaluation | Existing offline synthetic reports only. `fio`, ROC/PR evaluation, calibrated error bounds, and measured latency/throughput targets remain. |

No false-positive, detection-time, atomicity, throughput, or RPO target is claimed. See [scientific guardrails](docs/architecture.md) and [transactional lab semantics](docs/transactional-lab.md).

## Workspace layout

- `crates/aegis-classifier`: byte-distribution, header, correlation, and stride features.
- `crates/aegis-bench`: read-only paired-file benchmark and JSON export.
- `crates/aegis-journal`: original single-file laboratory journal plus grouped `TxJournal` transactions.
- `crates/aegis-sprt`: time-decayed evidence scorer. The lab probability profile is explicitly uncalibrated.
- `crates/aegis-control`: `/proc` lineage, explicitly scoped containment, and valid PDF/OOXML canaries.
- `crates/aegis-simulator`: safe corpus generator, gzip workload, RFC 8439 ChaCha20 workload, intermittent and multi-worker modes.
- `crates/aegis-daemon`: isolated lab state, simulator coordination, REST, WebSocket, and optional static console hosting.
- `crates/aegis-ctl`: ledger inspection and recovery CLI.
- `crates/aegis-replay`: offline classifier/scorer replay.
- `web`: React + TypeScript operational console and offline report viewer.
- `scripts`, `docs`, `tests`: synthetic corpus generation, design notes, and Python regression tests.

## Prerequisites

Linux, stable Rust/Cargo, Python 3.9+, Node.js 20+ and npm. `gzip` must be on `PATH` for the benign simulator workload. OpenSSL is needed only by the older synthetic corpus script. No FUSE device, root privileges, or eBPF support is required for this lab.

## Build and test

```sh
cargo test --workspace
python3 -m unittest discover -s tests -v
cd web
npm ci
npm run build
```

CI runs the same checks on every push and pull request.

## Run the live isolated-lab console

Build the console, then let the daemon serve it:

```sh
cd web && npm ci && npm run build && cd ..
cargo run -p aegis-daemon -- \
  --bind 0.0.0.0:8787 \
  --lab-root /tmp/aegisfs-lab \
  --web-root "$PWD/web/dist"
```

Open `http://localhost:8787`. In **Workload simulator**, start **Benign gzip**, **Full ChaCha20**, **Intermittent encryption**, **Multi-worker tree**, or **Canary tripwire**. The dashboard streams phase-space observations and SPRT-like trajectories; malicious profiles freeze the synthetic tree and replay the journal. A run is marked recovered only when the recursive lab file-tree BLAKE3 hash equals its pre-run value.

For Vite development instead:

```sh
cargo run -p aegis-daemon -- --lab-root /tmp/aegisfs-lab
cd web
npm run dev -- --port 5173
```

Vite proxies `/api` HTTP and WebSocket traffic to the daemon. The API is intentionally unauthenticated lab functionality; do not expose it as a production security service.

## Run the simulator without the UI

Choose fresh paths. Both directories may be absent or empty, and the command fails rather than deleting or reusing a nonempty lab:

```sh
cargo run -p aegis-simulator -- \
  --data-root /tmp/aegis-run-001/data \
  --journal-root /tmp/aegis-run-001/journal \
  --workload intermittent_encryption \
  --files 12 \
  --ragid cli-lab-001
```

The simulator uses fixed public test keys and generated data only. Full and multi-worker modes include rename/unlink operations. Intermittent mode transforms one 4 KiB block in four. Every run ends with inverse replay and a pre/post tree-hash check.

Inspect or recover a transaction group with unresolved operations:

```sh
cargo run -p aegis-ctl -- ledger \
  --data-root /path/to/lab/data --journal-root /path/to/lab/journal --ragid GROUP

cargo run -p aegis-ctl -- rollback \
  --data-root /path/to/lab/data --journal-root /path/to/lab/journal --ragid GROUP
```

## Lab API

- `GET /api/v1/status`, `/telemetry`, `/processes`, `/journal`, `/runs`, `/canaries`, `/alerts`
- `POST /api/v1/processes/:ragid/contain`
- `POST /api/v1/processes/:ragid/rollback`
- `POST /api/v1/canaries/deploy`
- `POST /api/v1/simulator/run` with `{"workload":"intermittent_encryption"}`
- `GET /api/v1/stream` with a WebSocket upgrade

Containment and rollback endpoints act on synthetic run state. They do not map RAGIDs to arbitrary host PIDs.

## Existing offline evaluation

The original paired-file benchmark and replay remain available:

```sh
python3 scripts/make_corpus.py synthetic-corpus --samples 10 --seed 2026
cargo run --release -p aegis-bench -- --manifest synthetic-corpus/manifest.csv --report synthetic.report.json
python3 scripts/summarize_report.py synthetic.report.json
```

Open the console’s **Offline research** view to load the report. See [synthetic evaluation](docs/synthetic-evaluation.md), [offline replay](docs/offline-replay.md), and [journal laboratory](docs/journal-lab.md) for schemas and limitations.

## Next implementation boundary

A production-oriented next milestone must add a real FUSE adapter with explicit mmap/rename/hard-link semantics, secure path resolution, crash/failure injection, concurrent-writer policy, physical-space accounting, and controlled performance tests. eBPF/LSM self-protection must be evaluated on supported kernels with a documented privileged-root threat model. Neither should be enabled by default until those invariants are tested.
