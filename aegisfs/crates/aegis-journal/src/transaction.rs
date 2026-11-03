//! Grouped, whole-file write-ahead transactions for the isolated laboratory.
//!
//! Every destructive operation persists verified pre-images before forwarding
//! the operation. Rollback replays inverse operations in reverse order and
//! refuses to overwrite a path whose current contents match neither the
//! recorded pre-image nor post-image. This module does not make multi-file
//! rollback atomic to concurrent readers and does not protect against hostile
//! path substitution or external writers.
use crate::filesystem::{
    digest, load_preimage, read_file, replace, store_preimage, sync_dir, target,
};
use anyhow::{bail, Context, Result};
use fs2::FileExt;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions, Permissions},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TxEntry {
    pub id: i64,
    pub ragid: String,
    pub timestamp_ns: u64,
    pub operation: String,
    pub path: String,
    pub new_path: Option<String>,
    pub pre_hash: Option<String>,
    pub post_hash: Option<String>,
    pub pre_bytes: u64,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct RecoveryReport {
    pub ragid: String,
    pub restored_operations: usize,
    pub restored_bytes: u64,
    pub verified: bool,
    pub entries: Vec<TxEntry>,
}

#[derive(Debug, Clone)]
struct Snapshot {
    exists: bool,
    hash: Option<String>,
    mode: u32,
    bytes: u64,
}

impl Snapshot {
    fn absent() -> Self {
        Self {
            exists: false,
            hash: None,
            mode: 0,
            bytes: 0,
        }
    }
}

#[derive(Debug, Clone)]
struct Record {
    entry: TxEntry,
    pre: Snapshot,
    pre_mode: u32,
    dest: Snapshot,
    dest_mode: u32,
    post_exists: bool,
}

pub struct TxJournal {
    root: PathBuf,
    objects: PathBuf,
    ragid: String,
    object_capacity: u64,
    db: Connection,
    _lock: File,
}

impl TxJournal {
    pub fn open(root: &Path, journal: &Path, ragid: &str) -> Result<Self> {
        Self::open_with_capacity(root, journal, ragid, 256 * 1024 * 1024)
    }

    pub fn open_with_capacity(
        root: &Path,
        journal: &Path,
        ragid: &str,
        object_capacity: u64,
    ) -> Result<Self> {
        validate_ragid(ragid)?;
        let root = root.canonicalize().context("resolve transaction data root")?;
        let journal = journal
            .canonicalize()
            .context("resolve transaction journal root")?;
        if !root.is_dir()
            || !journal.is_dir()
            || root.starts_with(&journal)
            || journal.starts_with(&root)
        {
            bail!("data and journal must be existing non-overlapping directories");
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(journal.join(format!("{ragid}.transaction.lock")))?;
        lock.try_lock_exclusive()
            .context("transaction group is already open")?;
        let objects = journal.join("objects");
        fs::create_dir_all(&objects)?;
        sync_dir(&journal)?;
        let db = Connection::open(journal.join("transactions.sqlite"))?;
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "synchronous", "FULL")?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS tx_ops (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ragid TEXT NOT NULL,
                timestamp_ns INTEGER NOT NULL,
                operation TEXT NOT NULL CHECK(operation IN ('WRITE','TRUNCATE','CREATE','RENAME','UNLINK')),
                path TEXT NOT NULL,
                new_path TEXT,
                pre_exists INTEGER NOT NULL,
                pre_hash TEXT,
                pre_mode INTEGER NOT NULL,
                pre_bytes INTEGER NOT NULL,
                dest_exists INTEGER NOT NULL,
                dest_hash TEXT,
                dest_mode INTEGER NOT NULL,
                post_exists INTEGER NOT NULL,
                post_hash TEXT,
                state TEXT NOT NULL CHECK(state IN ('PREPARED','APPLIED','ROLLED_BACK','COMMITTED'))
            );
            CREATE INDEX IF NOT EXISTS tx_group_order ON tx_ops(ragid, id DESC);
            CREATE INDEX IF NOT EXISTS tx_group_pending ON tx_ops(ragid, state, id DESC);",
        )?;
        Ok(Self {
            root,
            objects,
            ragid: ragid.to_owned(),
            object_capacity,
            db,
            _lock: lock,
        })
    }

    pub fn ragid(&self) -> &str {
        &self.ragid
    }

    pub fn object_usage(&self) -> Result<u64> {
        crate::filesystem::object_bytes(&self.objects)
    }

    pub fn write(&mut self, path: &str, bytes: &[u8]) -> Result<i64> {
        let operation = if self.capture(path)?.exists {
            "WRITE"
        } else {
            "CREATE"
        };
        let post = Snapshot {
            exists: true,
            hash: Some(digest(bytes)),
            mode: 0,
            bytes: bytes.len() as u64,
        };
        let id = self.prepare(operation, path, None, &post)?;
        let record = self.record(id)?.context("prepared write disappeared")?;
        self.check_source(&record)?;
        let mode = if record.pre.exists {
            record.pre_mode
        } else {
            0o644
        };
        let full = target(&self.root, path)?;
        replace(&full, bytes, Permissions::from_mode(mode))?;
        self.mark_applied(id)
    }

    pub fn truncate(&mut self, path: &str, length: usize) -> Result<i64> {
        let full = target(&self.root, path)?;
        let (current, _) = read_file(&full)?;
        if length > current.len() {
            bail!("laboratory truncate currently supports shrinking only");
        }
        let bytes = &current[..length];
        let post = Snapshot {
            exists: true,
            hash: Some(digest(bytes)),
            mode: 0,
            bytes: bytes.len() as u64,
        };
        let id = self.prepare("TRUNCATE", path, None, &post)?;
        let record = self.record(id)?.context("prepared truncate disappeared")?;
        self.check_source(&record)?;
        replace(&full, bytes, Permissions::from_mode(record.pre_mode))?;
        self.mark_applied(id)
    }

    pub fn unlink(&mut self, path: &str) -> Result<i64> {
        let post = Snapshot::absent();
        let id = self.prepare("UNLINK", path, None, &post)?;
        let record = self.record(id)?.context("prepared unlink disappeared")?;
        if !record.pre.exists {
            bail!("cannot unlink a nonexistent laboratory file");
        }
        self.check_source(&record)?;
        let full = target(&self.root, path)?;
        fs::remove_file(&full)?;
        sync_dir(full.parent().context("unlink path has no parent")?)?;
        self.mark_applied(id)
    }

    pub fn rename(&mut self, path: &str, new_path: &str) -> Result<i64> {
        if path == new_path {
            bail!("rename source and destination must differ");
        }
        let pre = self.capture(path)?;
        if !pre.exists {
            bail!("cannot rename a nonexistent laboratory file");
        }
        let post = Snapshot {
            exists: true,
            hash: pre.hash.clone(),
            mode: 0,
            bytes: pre.bytes,
        };
        let id = self.prepare("RENAME", path, Some(new_path), &post)?;
        let record = self.record(id)?.context("prepared rename disappeared")?;
        self.check_source(&record)?;
        self.check_dest(&record)?;
        let old = target(&self.root, path)?;
        let new = target(&self.root, new_path)?;
        fs::rename(&old, &new)?;
        sync_dir(old.parent().context("rename source has no parent")?)?;
        if old.parent() != new.parent() {
            sync_dir(new.parent().context("rename destination has no parent")?)?;
        }
        self.mark_applied(id)
    }

    fn prepare(
        &mut self,
        operation: &str,
        path: &str,
        new_path: Option<&str>,
        post: &Snapshot,
    ) -> Result<i64> {
        if self.pending_count()? != 0 {
            bail!("resolve pending transactions before applying new operations");
        }
        // Validate all paths before storing anything.
        target(&self.root, path)?;
        if let Some(new_path) = new_path {
            target(&self.root, new_path)?;
        }
        let pre = self.capture(path)?;
        let dest = match new_path {
            Some(path) => self.capture(path)?,
            None => Snapshot::absent(),
        };
        let timestamp_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock predates Unix epoch")?
            .as_nanos()
            .min(u64::MAX as u128) as u64;
        self.db.execute(
            "INSERT INTO tx_ops(
                ragid,timestamp_ns,operation,path,new_path,
                pre_exists,pre_hash,pre_mode,pre_bytes,
                dest_exists,dest_hash,dest_mode,post_exists,post_hash,state
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,'PREPARED')",
            params![
                self.ragid,
                timestamp_ns as i64,
                operation,
                path,
                new_path,
                pre.exists,
                pre.hash,
                pre.mode as i64,
                pre.bytes as i64,
                dest.exists,
                dest.hash,
                dest.mode as i64,
                post.exists,
                post.hash,
            ],
        )?;
        Ok(self.db.last_insert_rowid())
    }

    fn capture(&self, path: &str) -> Result<Snapshot> {
        let full = target(&self.root, path)?;
        let metadata = match fs::symlink_metadata(&full) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Snapshot::absent())
            }
            Err(error) => return Err(error.into()),
        };
        if !metadata.file_type().is_file() || metadata.nlink() != 1 {
            bail!("transaction target must be a regular file with one hard link");
        }
        let (bytes, permissions) = read_file(&full)?;
        let hash = store_preimage(&self.objects, &bytes, self.object_capacity)?;
        Ok(Snapshot {
            exists: true,
            hash: Some(hash),
            mode: permissions.mode(),
            bytes: bytes.len() as u64,
        })
    }

    fn mark_applied(&self, id: i64) -> Result<i64> {
        self.db.execute(
            "UPDATE tx_ops SET state='APPLIED' WHERE id=?1 AND state='PREPARED'",
            [id],
        )?;
        Ok(id)
    }

    fn pending_count(&self) -> Result<usize> {
        let count: i64 = self.db.query_row(
            "SELECT COUNT(*) FROM tx_ops WHERE ragid=?1 AND state='PREPARED'",
            [&self.ragid],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    fn current(&self, path: &str) -> Result<Snapshot> {
        let full = target(&self.root, path)?;
        let metadata = match fs::symlink_metadata(&full) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Snapshot::absent())
            }
            Err(error) => return Err(error.into()),
        };
        if !metadata.file_type().is_file() || metadata.nlink() != 1 {
            bail!("current target must be a regular file with one hard link");
        }
        let (bytes, _) = read_file(&full)?;
        Ok(Snapshot {
            exists: true,
            hash: Some(digest(&bytes)),
            mode: metadata.mode(),
            bytes: bytes.len() as u64,
        })
    }

    fn matches(current: &Snapshot, expected: &Snapshot) -> bool {
        current.exists == expected.exists && current.hash == expected.hash
    }

    fn check_source(&self, record: &Record) -> Result<()> {
        let current = self.current(&record.entry.path)?;
        if !Self::matches(&current, &record.pre) {
            bail!("conflict: source differs from its recorded pre-image");
        }
        Ok(())
    }

    fn check_dest(&self, record: &Record) -> Result<()> {
        if let Some(path) = &record.entry.new_path {
            let current = self.current(path)?;
            if !Self::matches(&current, &record.dest) {
                bail!("conflict: rename destination changed before apply");
            }
        }
        Ok(())
    }

    fn record(&self, id: i64) -> Result<Option<Record>> {
        self.db
            .query_row(
                "SELECT id,ragid,timestamp_ns,operation,path,new_path,
                    pre_exists,pre_hash,pre_mode,pre_bytes,
                    dest_exists,dest_hash,dest_mode,post_exists,post_hash,state
                 FROM tx_ops WHERE id=?1",
                [id],
                |row| record_from_row(row),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn ledger(&self) -> Result<Vec<TxEntry>> {
        let mut statement = self.db.prepare(
            "SELECT id,ragid,timestamp_ns,operation,path,new_path,
                pre_exists,pre_hash,pre_mode,pre_bytes,
                dest_exists,dest_hash,dest_mode,post_exists,post_hash,state
             FROM tx_ops WHERE ragid=?1 ORDER BY id DESC",
        )?;
        let rows = statement.query_map([&self.ragid], |row| {
            Ok(record_from_row(row)?.entry)
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn rollback(&mut self) -> Result<RecoveryReport> {
        let mut statement = self.db.prepare(
            "SELECT id,ragid,timestamp_ns,operation,path,new_path,
                pre_exists,pre_hash,pre_mode,pre_bytes,
                dest_exists,dest_hash,dest_mode,post_exists,post_hash,state
             FROM tx_ops WHERE ragid=?1 AND state IN ('PREPARED','APPLIED') ORDER BY id DESC",
        )?;
        let records = statement
            .query_map([&self.ragid], |row| record_from_row(row))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut restored_operations = 0;
        let mut restored_bytes = 0;
        for record in records {
            self.rollback_record(&record)?;
            self.db.execute(
                "UPDATE tx_ops SET state='ROLLED_BACK' WHERE id=?1 AND state IN ('PREPARED','APPLIED')",
                [record.entry.id],
            )?;
            restored_operations += 1;
            restored_bytes = restored_bytes.saturating_add(record.pre.bytes);
        }
        Ok(RecoveryReport {
            ragid: self.ragid.clone(),
            restored_operations,
            restored_bytes,
            verified: true,
            entries: self.ledger()?,
        })
    }

    fn rollback_record(&self, record: &Record) -> Result<()> {
        let path = target(&self.root, &record.entry.path)?;
        let current = self.current(&record.entry.path)?;
        let expected_post = Snapshot {
            exists: record.post_exists,
            hash: record.entry.post_hash.clone(),
            mode: 0,
            bytes: 0,
        };
        match record.entry.operation.as_str() {
            "WRITE" | "TRUNCATE" | "CREATE" => {
                if !Self::matches(&current, &record.pre)
                    && !Self::matches(&current, &expected_post)
                {
                    bail!("conflict: target matches neither pre-image nor post-image");
                }
                if !Self::matches(&current, &record.pre) {
                    self.restore(path, &record.pre, record.pre_mode)?;
                }
            }
            "UNLINK" => {
                if !Self::matches(&current, &expected_post)
                    && !Self::matches(&current, &record.pre)
                {
                    bail!("conflict: unlinked path was recreated by another actor");
                }
                if !current.exists {
                    self.restore(path, &record.pre, record.pre_mode)?;
                }
            }
            "RENAME" => {
                let new_path = record
                    .entry
                    .new_path
                    .as_deref()
                    .context("rename has no destination")?;
                let new_full = target(&self.root, new_path)?;
                let new_current = self.current(new_path)?;
                if !current.exists && Self::matches(&new_current, &expected_post) {
                    self.check_dest_ignoring_source(record)?;
                    fs::rename(&new_full, &path)?;
                    sync_dir(path.parent().context("rename restore has no parent")?)?;
                } else if !Self::matches(&current, &expected_post) {
                    bail!("conflict: renamed file matches neither expected location nor content");
                }
                let dest_now = self.current(new_path)?;
                if !Self::matches(&dest_now, &record.dest) {
                    self.restore(new_full, &record.dest, record.dest_mode)?;
                }
            }
            operation => bail!("unsupported journal operation {operation}"),
        }
        Ok(())
    }

    fn check_dest_ignoring_source(&self, record: &Record) -> Result<()> {
        let new_path = record
            .entry
            .new_path
            .as_deref()
            .context("rename has no destination")?;
        let current = self.current(new_path)?;
        let expected_post = Snapshot {
            exists: record.post_exists,
            hash: record.entry.post_hash.clone(),
            mode: 0,
            bytes: 0,
        };
        if !Self::matches(&current, &expected_post) {
            bail!("conflict: rename destination does not contain the recorded post-image");
        }
        Ok(())
    }

    fn restore(&self, path: PathBuf, snapshot: &Snapshot, mode: u32) -> Result<()> {
        if snapshot.exists {
            let hash = snapshot.hash.as_deref().context("missing pre-image hash")?;
            let bytes = load_preimage(&self.objects, hash)?;
            replace(&path, &bytes, Permissions::from_mode(mode))?;
        } else {
            match fs::remove_file(&path) {
                Ok(()) => sync_dir(path.parent().context("restore path has no parent")?)?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    pub fn commit(&self) -> Result<()> {
        if self.pending_count()? != 0 {
            bail!("cannot commit with prepared operations");
        }
        self.db.execute(
            "UPDATE tx_ops SET state='COMMITTED' WHERE ragid=?1 AND state='APPLIED'",
            [&self.ragid],
        )?;
        Ok(())
    }
}

fn record_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Record> {
    let pre_mode = row.get::<_, i64>(8)? as u32;
    let dest_mode = row.get::<_, i64>(12)? as u32;
    let pre = Snapshot {
        exists: row.get(6)?,
        hash: row.get(7)?,
        mode: pre_mode,
        bytes: row.get::<_, i64>(9)?.max(0) as u64,
    };
    let dest = Snapshot {
        exists: row.get(10)?,
        hash: row.get(11)?,
        mode: dest_mode,
        bytes: 0,
    };
    Ok(Record {
        entry: TxEntry {
            id: row.get(0)?,
            ragid: row.get(1)?,
            timestamp_ns: row.get::<_, i64>(2)?.max(0) as u64,
            operation: row.get(3)?,
            path: row.get(4)?,
            new_path: row.get(5)?,
            pre_hash: pre.hash.clone(),
            post_hash: row.get(14)?,
            pre_bytes: pre.bytes,
            state: row.get(15)?,
        },
        pre,
        pre_mode,
        dest,
        dest_mode,
        post_exists: row.get(13)?,
    })
}

fn validate_ragid(ragid: &str) -> Result<()> {
    if ragid.is_empty()
        || ragid.len() > 128
        || !ragid
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        bail!("invalid root ancestor group identifier");
    }
    Ok(())
}
