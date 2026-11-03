use aegis_journal::TxJournal;
use std::{fs, path::Path};
use tempfile::TempDir;

fn setup() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("data");
    let journal = temp.path().join("journal");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&journal).unwrap();
    (temp, root, journal)
}

fn read(root: &Path, name: &str) -> Vec<u8> {
    fs::read(root.join(name)).unwrap()
}

#[test]
fn reverse_replay_restores_write_rename_and_unlink() {
    let (_temp, root, log) = setup();
    fs::write(root.join("report"), b"original report").unwrap();
    let mut tx = TxJournal::open(&root, &log, "sim-test").unwrap();
    tx.write("report", b"encrypted report").unwrap();
    tx.rename("report", "report.locked").unwrap();
    tx.unlink("report.locked").unwrap();
    assert!(!root.join("report").exists());

    let report = tx.rollback().unwrap();
    assert_eq!(report.restored_operations, 3);
    assert!(report.verified);
    assert_eq!(read(&root, "report"), b"original report");
    assert!(!root.join("report.locked").exists());
    assert_eq!(tx.rollback().unwrap().restored_operations, 0);
}

#[test]
fn rename_over_existing_restores_both_files() {
    let (_temp, root, log) = setup();
    fs::write(root.join("source"), b"source before").unwrap();
    fs::write(root.join("destination"), b"destination before").unwrap();
    let mut tx = TxJournal::open(&root, &log, "sim-rename").unwrap();
    tx.write("source", b"source after").unwrap();
    tx.rename("source", "destination").unwrap();
    assert_eq!(read(&root, "destination"), b"source after");

    tx.rollback().unwrap();
    assert_eq!(read(&root, "source"), b"source before");
    assert_eq!(read(&root, "destination"), b"destination before");
}

#[test]
fn create_truncate_and_unlink_are_inverse_operations() {
    let (_temp, root, log) = setup();
    fs::write(root.join("document"), b"abcdefgh").unwrap();
    let mut tx = TxJournal::open(&root, &log, "sim-ops").unwrap();
    tx.truncate("document", 3).unwrap();
    tx.write("new-file", b"created").unwrap();
    tx.unlink("document").unwrap();
    assert!(!root.join("document").exists());

    tx.rollback().unwrap();
    assert_eq!(read(&root, "document"), b"abcdefgh");
    assert!(!root.join("new-file").exists());
}

#[test]
fn rollback_refuses_to_overwrite_a_concurrent_actor() {
    let (_temp, root, log) = setup();
    fs::write(root.join("document"), b"before").unwrap();
    let mut tx = TxJournal::open(&root, &log, "sim-conflict").unwrap();
    tx.write("document", b"after").unwrap();
    fs::write(root.join("document"), b"someone else").unwrap();

    let error = tx.rollback().unwrap_err().to_string();
    assert!(error.contains("conflict"), "{error}");
    assert_eq!(read(&root, "document"), b"someone else");
}

#[test]
fn preimage_survives_reopen_and_repeated_recovery() {
    let (_temp, root, log) = setup();
    fs::write(root.join("document"), b"durable before").unwrap();
    let id = {
        let mut tx = TxJournal::open(&root, &log, "sim-reopen").unwrap();
        tx.write("document", b"durable after").unwrap()
    };
    let mut tx = TxJournal::open(&root, &log, "sim-reopen").unwrap();
    assert!(tx.ledger().unwrap().iter().any(|entry| entry.id == id));
    tx.rollback().unwrap();
    assert_eq!(read(&root, "document"), b"durable before");
    tx.rollback().unwrap();
    assert_eq!(read(&root, "document"), b"durable before");
}

#[test]
fn capacity_is_fail_closed_before_data_changes() {
    let (_temp, root, log) = setup();
    fs::write(root.join("document"), b"1234567890").unwrap();
    let mut tx = TxJournal::open_with_capacity(&root, &log, "sim-capacity", 2).unwrap();
    assert!(tx.write("document", b"after").is_err());
    assert_eq!(read(&root, "document"), b"1234567890");
}
