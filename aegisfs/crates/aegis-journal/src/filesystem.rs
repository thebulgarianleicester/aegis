use anyhow::{bail, Context, Result};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
};

pub const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

/// Flat names only. A trusted isolated lab directory is still required:
/// validation alone is not a safe resolution primitive against path races.
pub fn validate_name(name: &str) -> Result<()> {
    let mut parts = Path::new(name).components();
    if name.is_empty()
        || name.starts_with('.')
        || !matches!(parts.next(), Some(Component::Normal(_)))
        || parts.next().is_some()
    {
        bail!("file name must be a single non-hidden component");
    }
    Ok(())
}

pub fn target(root: &Path, name: &str) -> Result<PathBuf> {
    validate_name(name)?;
    Ok(root.join(name))
}

pub fn read_file(path: &Path) -> Result<(Vec<u8>, fs::Permissions)> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("stat {}", path.display()))?;
    if !metadata.file_type().is_file() || metadata.nlink() != 1 {
        bail!("target must be a regular file with exactly one hard link");
    }
    if metadata.len() > MAX_FILE_BYTES {
        bail!("target exceeds 4 MiB laboratory limit");
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        bail!("target grew beyond limit");
    }
    Ok((bytes, metadata.permissions()))
}

pub fn digest(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

pub fn sync_dir(dir: &Path) -> Result<()> {
    File::open(dir)?
        .sync_all()
        .with_context(|| format!("sync directory {}", dir.display()))
}

/// Atomic visibility for one pathname on the same filesystem. This intentionally
/// does not promise preservation of inode identity, ACLs, xattrs or timestamps.
pub fn replace(path: &Path, bytes: &[u8], permissions: fs::Permissions) -> Result<()> {
    let parent = path.parent().context("target has no parent")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.as_file().set_permissions(permissions)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path)
        .map_err(|err| err.error)
        .context("rename replacement")?;
    sync_dir(parent)?;
    Ok(())
}

/// Counts every recognized object, including unreferenced/partial objects.
/// Unexpected entries and oversized objects fail closed rather than being ignored.
pub fn object_bytes(dir: &Path) -> Result<u64> {
    let mut total = 0u64;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().context("invalid object filename")?;
        if !valid_hash(name) {
            bail!("unexpected object store entry");
        }
        let meta = fs::symlink_metadata(entry.path())?;
        if !meta.file_type().is_file() || meta.nlink() != 1 || meta.len() > MAX_FILE_BYTES {
            bail!("invalid preimage object metadata");
        }
        total = total
            .checked_add(meta.len())
            .context("object usage overflow")?;
    }
    Ok(total)
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

pub fn store_preimage(dir: &Path, bytes: &[u8], capacity: u64) -> Result<String> {
    let hash = digest(bytes);
    let path = dir.join(&hash);
    let used = object_bytes(dir)?;
    // Only a new object consumes additional logical bytes. This quota does not
    // reserve physical disk space and excludes SQLite, temporary files, etc.
    let additional = match fs::symlink_metadata(&path) {
        Ok(_) => 0,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => bytes.len() as u64,
        Err(e) => return Err(e.into()),
    };
    let required = used
        .checked_add(additional)
        .context("object usage overflow")?;
    if required > capacity {
        bail!("preimage capacity exceeded: need {required} bytes, limit {capacity}");
    }
    // A failed/interrupted create can leave a short object. Do not accept it.
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut f) => {
            f.write_all(bytes)?;
            f.sync_all()?;
            sync_dir(dir)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let (existing, _) = read_file(&path)?;
            if digest(&existing) != hash {
                bail!("existing preimage object failed hash verification");
            }
            // A previous writer could have stopped after creating a complete
            // object but before syncing it. Re-sync before SQLite references it.
            File::open(&path)?.sync_all()?;
            sync_dir(dir)?;
        }
        Err(e) => return Err(e.into()),
    }
    Ok(hash)
}

pub fn load_preimage(dir: &Path, hash: &str) -> Result<Vec<u8>> {
    // Hash is loaded from the database; reject malformed or path-like data.
    if !valid_hash(hash) {
        bail!("invalid preimage hash");
    }
    let (bytes, _) = read_file(&dir.join(hash))?;
    if digest(&bytes) != hash {
        bail!("preimage object failed hash verification");
    }
    Ok(bytes)
}
