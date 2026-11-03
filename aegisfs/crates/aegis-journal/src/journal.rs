use crate::filesystem::{
    digest, load_preimage, object_bytes, read_file, replace, store_preimage, sync_dir, target,
    MAX_FILE_BYTES,
};
use anyhow::{bail, Context, Result};
use fs2::FileExt;
use rusqlite::{params, Connection, OptionalExtension};
use std::{
    fs::{self, File, OpenOptions},
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Prepared,
    Applied,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub id: i64,
    pub name: String,
    pub phase: Phase,
    pub pre_hash: String,
    pub post_hash: String,
}

pub struct Journal {
    root: PathBuf,
    objects: PathBuf,
    object_capacity: u64,
    db: Connection,
    _lock: File,
}

impl Journal {
    /// Opens pre-existing, separate, trusted directories and takes a cooperative
    /// exclusive lock. Nothing prevents an external non-cooperating writer.
    pub fn open(root: &Path, journal: &Path) -> Result<Self> {
        Self::open_with_capacity(root, journal, 64 * 1024 * 1024)
    }

    /// Sets the maximum logical bytes in preimage objects for this session.
    /// Capacity is checked only for new prepares, so recovery remains possible
    /// when reopened with a lower limit. This does not reserve physical space.
    pub fn open_with_capacity(root: &Path, journal: &Path, object_capacity: u64) -> Result<Self> {
        let root = root
            .canonicalize()
            .context("resolve laboratory data root")?;
        let journal = journal
            .canonicalize()
            .context("resolve laboratory journal root")?;
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
            .open(journal.join("writer.lock"))?;
        lock.try_lock_exclusive()
            .context("journal is already open by another writer")?;
        let objects = journal.join("objects");
        fs::create_dir_all(&objects)?;
        sync_dir(&journal)?;
        let db = Connection::open(journal.join("journal.sqlite"))?;
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "synchronous", "FULL")?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS writes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            pre_hash TEXT NOT NULL,
            post_hash TEXT NOT NULL,
            state TEXT NOT NULL CHECK(state IN ('PREPARED','APPLIED','ROLLED_BACK','COMMITTED'))
        );
        CREATE UNIQUE INDEX IF NOT EXISTS single_pending ON writes ((1))
          WHERE state IN ('PREPARED','APPLIED');
        CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value BLOB NOT NULL);",
        )?;
        // Bind a journal to one canonical directory and inode to prevent an
        // accidentally reused log from rolling back a different laboratory root.
        let metadata = fs::metadata(&root)?;
        let mut identity = format!("{}:{}:", metadata.dev(), metadata.ino()).into_bytes();
        identity.extend_from_slice(root.as_os_str().as_bytes());
        let stored: Option<Vec<u8>> = db
            .query_row(
                "SELECT value FROM settings WHERE key='data_root'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        match stored {
            Some(value) if value != identity => bail!("journal belongs to a different data root"),
            Some(_) => {}
            None => {
                let count: i64 = db.query_row("SELECT COUNT(*) FROM writes", [], |r| r.get(0))?;
                if count != 0 {
                    bail!("refusing to bind an unbound nonempty journal");
                }
                db.execute(
                    "INSERT INTO settings(key,value) VALUES('data_root',?1)",
                    [&identity],
                )?;
            }
        }
        sync_dir(&journal)?;
        Ok(Self {
            root,
            objects,
            object_capacity,
            db,
            _lock: lock,
        })
    }

    /// Logical bytes of objects currently in the store, including orphans.
    pub fn object_usage(&self) -> Result<u64> {
        object_bytes(&self.objects)
    }

    pub fn pending(&self) -> Result<Option<Pending>> {
        self.db.query_row("SELECT id,name,pre_hash,post_hash,state FROM writes WHERE state IN ('PREPARED','APPLIED') ORDER BY id LIMIT 1", [], |r| {
            let state: String = r.get(4)?;
            Ok(Pending { id:r.get(0)?, name:r.get(1)?, pre_hash:r.get(2)?, post_hash:r.get(3)?, phase:if state == "PREPARED" { Phase::Prepared } else { Phase::Applied } })
        }).optional().map_err(Into::into)
    }

    /// Persist a verified original object before committing a PREPARED record.
    /// The caller must retain post bytes until apply. After restart rollback can
    /// be attempted only if the required data and conflict checks still pass.
    pub fn prepare(&mut self, name: &str, post: &[u8]) -> Result<i64> {
        if self.pending()?.is_some() {
            bail!("resolve the existing pending transaction first");
        }
        if post.len() as u64 > MAX_FILE_BYTES {
            bail!("replacement exceeds 4 MiB laboratory limit");
        }
        let path = target(&self.root, name)?;
        let (pre, _) = read_file(&path)?;
        let pre_hash = store_preimage(&self.objects, &pre, self.object_capacity)?;
        let post_hash = digest(post);
        self.db.execute(
            "INSERT INTO writes(name,pre_hash,post_hash,state) VALUES(?1,?2,?3,'PREPARED')",
            params![name, pre_hash, post_hash],
        )?;
        Ok(self.db.last_insert_rowid())
    }

    pub fn apply(&mut self, id: i64, post: &[u8]) -> Result<()> {
        self.apply_inner(id, post, || Ok(()))
    }

    fn apply_inner<F: FnOnce() -> Result<()>>(
        &mut self,
        id: i64,
        post: &[u8],
        after_replace: F,
    ) -> Result<()> {
        let pending = self.pending()?.context("no pending transaction")?;
        if pending.id != id || pending.phase != Phase::Prepared {
            bail!("transaction is not PREPARED");
        }
        if post.len() as u64 > MAX_FILE_BYTES || digest(post) != pending.post_hash {
            bail!("replacement does not match prepared hash");
        }
        // Refuse the destructive replacement if the persisted preimage was lost
        // or corrupted after preparation.
        let saved = load_preimage(&self.objects, &pending.pre_hash)?;
        let path = target(&self.root, &pending.name)?;
        let (current, permissions) = read_file(&path)?;
        if digest(&current) != pending.pre_hash || current != saved {
            bail!("conflict: target differs from prepared preimage");
        }
        replace(&path, post, permissions)?;
        if digest(&read_file(&path)?.0) != pending.post_hash {
            bail!("replacement verification failed");
        }
        // Testable crash point: file is durable while DB may still say PREPARED.
        after_replace()?;
        self.db.execute(
            "UPDATE writes SET state='APPLIED' WHERE id=?1 AND state='PREPARED'",
            [id],
        )?;
        Ok(())
    }

    /// Undo a pending operation, refusing to overwrite an unknown current value.
    /// Only byte contents are verified. Metadata restoration is unsupported.
    pub fn rollback(&mut self, id: i64) -> Result<()> {
        self.rollback_inner(id, || Ok(()))
    }
    fn rollback_inner<F: FnOnce() -> Result<()>>(
        &mut self,
        id: i64,
        after_replace: F,
    ) -> Result<()> {
        let pending = self.pending()?.context("no pending transaction")?;
        if pending.id != id {
            bail!("not the pending transaction");
        }
        let path = target(&self.root, &pending.name)?;
        let (current, permissions) = read_file(&path)?;
        let current_hash = digest(&current);
        // Always verify the object, even if file already looks restored.
        let pre = load_preimage(&self.objects, &pending.pre_hash)?;
        if current_hash == pending.post_hash && current_hash != pending.pre_hash {
            replace(&path, &pre, permissions)?;
            if digest(&read_file(&path)?.0) != pending.pre_hash {
                bail!("restored data verification failed");
            }
            after_replace()?;
        } else if current_hash != pending.pre_hash {
            bail!("conflict: target matches neither preimage nor expected postimage");
        }
        self.db.execute(
            "UPDATE writes SET state='ROLLED_BACK' WHERE id=?1 AND state IN ('PREPARED','APPLIED')",
            [id],
        )?;
        Ok(())
    }

    /// Finalize an applied transaction without rollback. Preimage objects are
    /// currently retained indefinitely and counted against the object quota.
    pub fn commit(&mut self, id: i64) -> Result<()> {
        let pending = self.pending()?.context("no pending transaction")?;
        if pending.id != id || pending.phase != Phase::Applied {
            bail!("transaction is not APPLIED");
        }
        let (current, _) = read_file(&target(&self.root, &pending.name)?)?;
        if digest(&current) != pending.post_hash {
            bail!("conflict: target differs from postimage");
        }
        self.db.execute(
            "UPDATE writes SET state='COMMITTED' WHERE id=?1 AND state='APPLIED'",
            [id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use tempfile::TempDir;

    fn setup() -> (TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("data");
        let log = dir.path().join("journal");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&log).unwrap();
        fs::write(root.join("file"), b"original").unwrap();
        (dir, root, log)
    }
    #[test]
    fn crash_after_file_rename_before_database_update() {
        let (_dir, root, log) = setup();
        let mut j = Journal::open(&root, &log).unwrap();
        let id = j.prepare("file", b"replacement").unwrap();
        assert!(j
            .apply_inner(id, b"replacement", || Err(anyhow!(
                "simulated interruption"
            )))
            .is_err());
        assert_eq!(j.pending().unwrap().unwrap().phase, Phase::Prepared);
        drop(j);
        let mut j = Journal::open(&root, &log).unwrap();
        j.rollback(id).unwrap();
        assert_eq!(fs::read(root.join("file")).unwrap(), b"original");
        assert!(j.pending().unwrap().is_none());
    }
    #[test]
    fn crash_after_rollback_rename_before_database_update() {
        let (_dir, root, log) = setup();
        let mut j = Journal::open(&root, &log).unwrap();
        let id = j.prepare("file", b"replacement").unwrap();
        j.apply(id, b"replacement").unwrap();
        assert!(j
            .rollback_inner(id, || Err(anyhow!("simulated interruption")))
            .is_err());
        drop(j);
        let mut j = Journal::open(&root, &log).unwrap();
        j.rollback(id).unwrap();
        assert_eq!(fs::read(root.join("file")).unwrap(), b"original");
        assert!(j.pending().unwrap().is_none());
    }
    #[test]
    fn sqlite_prepare_failure_leaves_unmodified_target_and_reusable_orphan() {
        let (_dir, root, log) = setup();
        let mut j = Journal::open_with_capacity(&root, &log, b"original".len() as u64).unwrap();
        j.db.execute_batch("CREATE TRIGGER reject_prepare BEFORE INSERT ON writes BEGIN SELECT RAISE(ABORT,'injected database failure'); END;").unwrap();
        assert!(j.prepare("file", b"replacement").is_err());
        assert!(j.pending().unwrap().is_none());
        assert_eq!(fs::read(root.join("file")).unwrap(), b"original");
        assert_eq!(j.object_usage().unwrap(), b"original".len() as u64);
        j.db.execute_batch("DROP TRIGGER reject_prepare").unwrap();
        let id = j.prepare("file", b"replacement").unwrap();
        j.apply(id, b"replacement").unwrap();
        j.rollback(id).unwrap();
        assert_eq!(fs::read(root.join("file")).unwrap(), b"original");
    }

    #[test]
    fn sqlite_apply_failure_after_replace_is_recoverable() {
        let (_dir, root, log) = setup();
        let mut j = Journal::open(&root, &log).unwrap();
        let id = j.prepare("file", b"replacement").unwrap();
        j.db.execute_batch("CREATE TRIGGER reject_applied BEFORE UPDATE ON writes WHEN NEW.state='APPLIED' BEGIN SELECT RAISE(ABORT,'injected database failure'); END;").unwrap();
        assert!(j.apply(id, b"replacement").is_err());
        assert_eq!(fs::read(root.join("file")).unwrap(), b"replacement");
        assert_eq!(j.pending().unwrap().unwrap().phase, Phase::Prepared);
        drop(j);
        let mut j = Journal::open(&root, &log).unwrap();
        j.rollback(id).unwrap();
        assert_eq!(fs::read(root.join("file")).unwrap(), b"original");
        assert!(j.pending().unwrap().is_none());
    }
}
