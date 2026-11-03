use aegis_journal::{Journal, Phase};
use std::{fs, os::unix::fs::symlink, path::PathBuf};
use tempfile::TempDir;

fn setup() -> (TempDir, PathBuf, PathBuf) {
    let base = tempfile::tempdir().unwrap();
    let data = base.path().join("data");
    let log = base.path().join("log");
    fs::create_dir(&data).unwrap();
    fs::create_dir(&log).unwrap();
    fs::write(data.join("doc"), b"original content").unwrap();
    (base, data, log)
}
#[test]
fn prepare_apply_rollback_and_commit() {
    let (_base, data, log) = setup();
    let mut journal = Journal::open(&data, &log).unwrap();
    let id = journal.prepare("doc", b"replacement").unwrap();
    assert_eq!(journal.pending().unwrap().unwrap().phase, Phase::Prepared);
    assert_eq!(fs::read(data.join("doc")).unwrap(), b"original content");
    assert!(journal.prepare("doc", b"other").is_err());
    assert!(journal.apply(id, b"wrong bytes").is_err());
    journal.apply(id, b"replacement").unwrap();
    assert_eq!(fs::read(data.join("doc")).unwrap(), b"replacement");
    journal.rollback(id).unwrap();
    assert_eq!(fs::read(data.join("doc")).unwrap(), b"original content");
    let id = journal.prepare("doc", b"accepted").unwrap();
    journal.apply(id, b"accepted").unwrap();
    journal.commit(id).unwrap();
    assert!(journal.pending().unwrap().is_none());
    assert_eq!(fs::read(data.join("doc")).unwrap(), b"accepted");
}
#[test]
fn recovery_before_apply_and_exclusive_lock() {
    let (_base, data, log) = setup();
    let mut journal = Journal::open(&data, &log).unwrap();
    let id = journal.prepare("doc", b"replacement").unwrap();
    assert!(Journal::open(&data, &log).is_err());
    drop(journal);
    let mut reopened = Journal::open(&data, &log).unwrap();
    assert_eq!(reopened.pending().unwrap().unwrap().id, id);
    reopened.rollback(id).unwrap();
    assert_eq!(fs::read(data.join("doc")).unwrap(), b"original content");
}
#[test]
fn external_changes_are_not_overwritten() {
    let (_base, data, log) = setup();
    let mut journal = Journal::open(&data, &log).unwrap();
    let id = journal.prepare("doc", b"replacement").unwrap();
    journal.apply(id, b"replacement").unwrap();
    fs::write(data.join("doc"), b"external change").unwrap();
    assert!(journal.rollback(id).is_err());
    assert!(journal.commit(id).is_err());
    assert_eq!(fs::read(data.join("doc")).unwrap(), b"external change");
    assert_eq!(journal.pending().unwrap().unwrap().id, id);
}
#[test]
fn rejects_paths_symlinks_and_hardlinks() {
    let (_base, data, log) = setup();
    let mut journal = Journal::open(&data, &log).unwrap();
    for name in ["../doc", "nested/doc", ".", "", ".hidden"] {
        assert!(journal.prepare(name, b"new").is_err());
    }
    symlink("doc", data.join("link")).unwrap();
    assert!(journal.prepare("link", b"new").is_err());
    fs::hard_link(data.join("doc"), data.join("hardlink")).unwrap();
    assert!(journal.prepare("doc", b"new").is_err());
}
#[test]
fn corruption_blocks_apply_and_rollback() {
    let (_base, data, log) = setup();
    let mut journal = Journal::open(&data, &log).unwrap();
    let id = journal.prepare("doc", b"replacement").unwrap();
    let hash = journal.pending().unwrap().unwrap().pre_hash;
    fs::write(log.join("objects").join(hash), b"bad object").unwrap();
    assert!(journal.apply(id, b"replacement").is_err());
    assert!(journal.rollback(id).is_err());
    assert_eq!(fs::read(data.join("doc")).unwrap(), b"original content");
}

#[test]
fn journal_is_bound_to_one_data_root() {
    let (base, data, log) = setup();
    let other = base.path().join("other");
    fs::create_dir(&other).unwrap();
    fs::write(other.join("doc"), b"original content").unwrap();
    let mut journal = Journal::open(&data, &log).unwrap();
    journal.prepare("doc", b"replacement").unwrap();
    drop(journal);
    assert!(Journal::open(&other, &log).is_err());
    assert!(Journal::open(&data, &log)
        .unwrap()
        .pending()
        .unwrap()
        .is_some());
    assert_eq!(fs::read(other.join("doc")).unwrap(), b"original content");
}

#[test]
fn crash_child_entry() {
    let Ok(stage) = std::env::var("AEGIS_CRASH_STAGE") else {
        return;
    };
    let data = PathBuf::from(std::env::var_os("AEGIS_TEST_DATA").unwrap());
    let log = PathBuf::from(std::env::var_os("AEGIS_TEST_LOG").unwrap());
    let mut journal = Journal::open(&data, &log).unwrap();
    let id = journal.prepare("doc", b"replacement").unwrap();
    if stage == "applied" {
        journal.apply(id, b"replacement").unwrap();
    }
    // Abrupt process termination without running Rust destructors.
    std::process::exit(73);
}

#[test]
fn abrupt_process_exit_recovers_prepared_and_applied() {
    for stage in ["prepared", "applied"] {
        let (_base, data, log) = setup();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "crash_child_entry", "--nocapture"])
            .env("AEGIS_CRASH_STAGE", stage)
            .env("AEGIS_TEST_DATA", &data)
            .env("AEGIS_TEST_LOG", &log)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(73));
        let mut journal = Journal::open(&data, &log).unwrap();
        let pending = journal.pending().unwrap().unwrap();
        assert_eq!(
            pending.phase,
            if stage == "prepared" {
                Phase::Prepared
            } else {
                Phase::Applied
            }
        );
        journal.rollback(pending.id).unwrap();
        assert_eq!(fs::read(data.join("doc")).unwrap(), b"original content");
    }
}

#[test]
fn capacity_is_fail_closed_without_eviction_and_allows_deduplication() {
    let (_base, data, log) = setup();
    let cap = b"original content".len() as u64;
    fs::write(data.join("same"), b"original content").unwrap();
    fs::write(data.join("other"), b"different data").unwrap();
    let mut journal = Journal::open_with_capacity(&data, &log, cap).unwrap();
    let first = journal.prepare("doc", b"one").unwrap();
    journal.apply(first, b"one").unwrap();
    journal.commit(first).unwrap();
    assert_eq!(journal.object_usage().unwrap(), cap);
    let identical = journal.prepare("same", b"two").unwrap();
    journal.apply(identical, b"two").unwrap();
    journal.commit(identical).unwrap();
    assert_eq!(journal.object_usage().unwrap(), cap);
    assert!(journal.prepare("other", b"three").is_err());
    assert_eq!(fs::read(data.join("other")).unwrap(), b"different data");
    assert!(journal.pending().unwrap().is_none());
    assert_eq!(fs::read_dir(log.join("objects")).unwrap().count(), 1);
}

#[test]
fn lower_capacity_does_not_prevent_recovery_of_pending_transaction() {
    let (_base, data, log) = setup();
    let mut journal = Journal::open_with_capacity(&data, &log, 100).unwrap();
    let id = journal.prepare("doc", b"changed").unwrap();
    journal.apply(id, b"changed").unwrap();
    drop(journal);
    let mut reopened = Journal::open_with_capacity(&data, &log, 0).unwrap();
    reopened.rollback(id).unwrap();
    assert_eq!(fs::read(data.join("doc")).unwrap(), b"original content");
    assert!(reopened.prepare("doc", b"second").is_err());
}

#[test]
fn unexpected_or_oversized_store_entries_fail_closed() {
    let (_base, data, log) = setup();
    let mut journal = Journal::open_with_capacity(&data, &log, 100).unwrap();
    fs::write(log.join("objects").join("unexpected"), b"garbage").unwrap();
    assert!(journal.object_usage().is_err());
    assert!(journal.prepare("doc", b"replacement").is_err());
    assert_eq!(fs::read(data.join("doc")).unwrap(), b"original content");
    fs::remove_file(log.join("objects").join("unexpected")).unwrap();
    let oversized = fs::File::create(log.join("objects").join("a".repeat(64))).unwrap();
    oversized.set_len(4 * 1024 * 1024 + 1).unwrap();
    assert!(journal.object_usage().is_err());
    assert!(journal.prepare("doc", b"replacement").is_err());
}
