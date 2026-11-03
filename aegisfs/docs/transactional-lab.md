# Transactional laboratory semantics

## Scope

`TxJournal` is a durable, grouped, whole-file write-ahead journal for trusted, isolated lab directories. It is the recovery engine used by the safe simulator and `aegis-ctl`. It does **not** intercept VFS operations and is not presented as a FUSE filesystem.

## Write-ahead ordering

For each operation, the journal:

1. Validates the relative path and inspects the regular-file target.
2. Writes and `fsync`s a BLAKE3-addressed pre-image object before SQLite references it.
3. Persists a `PREPARED` operation in SQLite WAL mode with `synchronous=FULL`.
4. Applies the filesystem operation and syncs affected directories where applicable.
5. Marks the operation `APPLIED`.

A pending group must resolve its prepared operations before new operations are accepted. Recovery includes both `PREPARED` and `APPLIED` records, covering interruption after the filesystem change but before the state update. Whole-file replacement uses a same-directory temporary file, file sync, atomic rename, and parent-directory sync.

## Supported inverse operations

- `WRITE` / `CREATE`: replace the post-image with the original file or remove a newly created file.
- `TRUNCATE`: restore the complete original file (the current API supports shrinking only).
- `RENAME`: move the new path back to the old path. If the rename replaced an existing destination, restore that destination from its own saved pre-image.
- `UNLINK`: recreate the original file and bytes.

Replay is strictly reverse ordered by transaction ID. Repeating recovery is idempotent for an unchanged lab. File permissions are restored; inode identity, hard links, ACLs, xattrs, timestamps, sparse layout, and directory metadata are not.

## Conflict behavior

Before replacing or removing a path, recovery checks its current existence and BLAKE3 digest against recorded states. A path changed by another actor causes an error instead of being silently overwritten. Rename recovery handles the source and replaced destination independently.

This is a cooperative conflict check, not a race-free lock against hostile writers. Path substitution, symlinks, hard links, external writers, and concurrent path changes remain outside the trusted-lab boundary.

## What “verified recovery” means in the UI

The simulator hashes every generated filename and file byte before the workload and again after rollback. The UI shows `BLAKE3 TREE VERIFIED` only when those tree hashes match. This is strong evidence for the generated flat lab corpus, not a general RPO proof for arbitrary filesystems.

Multi-file recovery is **not atomic to readers**. The journal directory must be separate from the data directory. The logical pre-image quota fails new preparation when exceeded, but does not reserve physical storage or account for SQLite and temporary-file overhead. Existing objects are retained, including committed/orphaned objects.

## Current limits

- Regular files with exactly one hard link.
- Maximum 4 MiB per whole-file pre-image, inherited from the laboratory store.
- Existing parent directories; no recursive directory transaction support.
- Trusted path resolution without concurrent adversarial renaming.
- Cooperative single writer per RAGID lock; no general distributed transaction semantics.
- No inline FUSE interception, mmap tracking, fanotify mediation, or kernel enforcement.

Future kernel-boundary work must address these limits before making data-loss or containment guarantees.
