# A2 journal laboratory prototype

`crates/aegis-journal` is a **research-only library**, not a mounted filesystem, not a daemon, and not a supported backup tool. It is not integrated with the classifier or console and does not monitor filesystem operations. Exercise it only in isolated, disposable, trusted test directories; it is not safe for user files. Run `cargo test -p aegis-journal`.

## Scope

One cooperative writer per journal and one unresolved whole-file operation at a time. A target must already exist, be a regular single-hardlink file, be a single non-hidden pathname component and be at most 4 MiB. Replacement data have the same size limit. Whole-file BLAKE3-addressed preimages are stored in a separate journal directory; these are **not 4 KiB extents or a ring buffer**. Objects are not garbage-collected or encrypted. A per-session logical object-byte quota (64 MiB by default) is configurable with `open_with_capacity`; it does not reserve physical disk space. A journal is bound to the canonical data-root path and its device/inode, and its lock only coordinates processes opening the **same journal directory**. It does not prevent another journal from targeting the same root or prevent non-cooperating writers.

## Capacity policy

Before preparing a new operation, the store scans recognized object files and sums their logical lengths, including orphan and partial objects. A new preimage is refused before the target is changed if it would exceed the configured object-byte limit; existing verified identical preimages are deduplicated. No objects are evicted. Unexpected filenames, symlinks, hard links and oversized objects cause the scan to fail closed. The policy is for a trusted, cooperatively locked laboratory directory and is subject to the path races described below.

The limit is chosen for each open session and is not persisted; reopening with a lower limit can prevent new prepares but does not prevent rollback of a pending operation. SQLite/WAL files, temporary replacements, filesystem allocation overhead and disk space used outside the object directory are **not** counted. Actual ENOSPC/EIO may still occur at any point; an apply error after rename can mean the target changed while the record remains `PREPARED`, and the caller must inspect/recover the pending operation. An orphan consumes quota until manually investigated; there is no automated garbage collection or quota resize workflow.

## Ordering and states

1. `prepare`: validate/read the target, save its BLAKE3 preimage, sync its file and directory, then insert a `PREPARED` record into SQLite WAL configured with `synchronous=FULL`. Reused objects are reverified and re-synced before being referenced. Postimage bytes are not retained, only their hash. No target change occurs during this step.
2. `apply`: check the preimage object and current target against the recorded preimage and verify the supplied postimage hash. Write a same-directory temporary file, sync it, replace the target with a single-file rename, sync the directory, verify read-back, then record `APPLIED`.
3. `rollback`: verify the preimage object; if the current content is the expected postimage, replace and sync it with the preimage; if it is already the preimage, leave it alone. Verify restored bytes and record `ROLLED_BACK`. A different current content is treated as a conflict and left untouched. For `PREPARED`, either pre or post may be present after interruption. `commit` instead finalizes an applied replacement after a hash check.

Only one unresolved row is allowed by both the API and a SQLite partial unique index. A prepared operation can be rolled back after reopening without the original postimage; applying it after reopening requires the exact postimage bytes from the caller. A conflict or a corrupted/missing preimage stops recovery and requires manual investigation; neither is silently overwritten. A failed attempt to create an object can leave a partial orphan, which causes subsequent reuse of that hash to fail closed. Interrupted replacement can leave temporary orphan files. There is no automatic cleanup.

## Tests and limits of evidence

Tests cover the ordinary sequence, commit, reopen before apply, cooperative writer exclusion, accidental use of a different data root, rejecting symlinks/hard links/path traversal, conflict refusal, preimage corruption, capacity exhaustion, deduplication, recovery while over the newly configured limit, and unexpected store entries. A proptest state machine generates mixed single-file operations and clean reopens. Tests also inject SQLite errors at preparation and after replacement, simulate interruptions after replacement/rollback renames and before DB updates, and terminate subprocesses abruptly after `PREPARED` and `APPLIED`. These are process-level and injected-error tests, not power-cut tests. There is no evidence yet that the ordering survives filesystem, hardware, kernel, SQLite, or device-specific failures. The device/inode/path binding can reject legitimate root moves or remounts; it is not a cryptographic identity.

Only byte contents are checked/restored. The prototype replaces the inode and copies the *currently observed* Unix mode onto the replacement. It does not preserve or restore ownership, ACLs, xattrs, timestamps, sparse layout, reflinks, open file descriptor semantics, mappings, hard links or extended metadata. It cannot undo creates, unlinks, renames, truncates, directory operations or multiple files atomically. There is no PID attribution, process-tree rollback, FUSE, fanotify, eBPF, or tamper resistance.

Hash matching is not sufficient for adversarial or concurrent conflict resolution: an external writer can race between check and rename, and another write with equal bytes is indistinguishable. Flat-name validation and `symlink_metadata` do not eliminate path substitution races. The entire design assumes trusted directories with no independent writer. `flock` is advisory, only covers a particular journal, and is not a kernel I/O gate. No isolation, recovery-completeness, zero-data-loss, availability, security or throughput guarantee is made.

## Before FUSE integration

- Design and implement safe directory-fd-based resolution (`openat2` or equivalent), namespace/object identity, trusted journal ownership and capability boundaries; handle moves and remounts explicitly.
- Define concurrency and version/conflict semantics that protect other writers, including equal-content writes, open descriptors and mappings.
- Specify metadata, creates, unlinks, renames, truncation, sparse extents, hard links and multi-file visibility; add model-based/property tests.
- Move from a logical in-process object quota to durable physical-space reservation where supported, safe cleanup/restart handling for orphans, and richer recovery-time corruption reporting.
- Run subprocess failpoints at each persistence boundary, fault injection for ENOSPC/EIO, actual crash/power-failure tests on supported filesystems and independently verify directory trees and metadata.
- Only then prototype FUSE in a disposable mount namespace, with documented bypasses, privilege requirements and an explicit non-production status.
