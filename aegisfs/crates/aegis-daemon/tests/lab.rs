use aegis_daemon::AppState;
use std::{thread, time::Duration};

fn wait_for_completion(state: &AppState) -> aegis_daemon::RunView {
    for _ in 0..500 {
        let telemetry = state.telemetry().unwrap();
        if let Some(run) = telemetry.runs.first() {
            if run.status.ends_with("RECOVERED") || run.status == "FAILED" {
                return run.clone();
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("simulator did not finish");
}

#[test]
fn full_encryption_is_contained_and_directory_is_restored() {
    let temp = tempfile::tempdir().unwrap();
    let state = AppState::new(temp.path()).unwrap();
    state.start_simulation("full_encryption").unwrap();
    let run = wait_for_completion(&state);
    assert_eq!(run.status, "CONTAINED_RECOVERED", "{run:?}");
    assert!(run.contained);
    assert!(run.recovered);
    assert_eq!(run.pre_tree_blake3, run.post_tree_blake3);
    assert!(run.touched_files <= 5);
    let telemetry = state.telemetry().unwrap();
    assert!(!telemetry.ledger.is_empty());
    assert!(telemetry
        .processes
        .iter()
        .any(|process| process.status == "RECOVERED"));
}

#[test]
fn intermittent_encryption_is_contained_and_restored() {
    let temp = tempfile::tempdir().unwrap();
    let state = AppState::new(temp.path()).unwrap();
    state.start_simulation("intermittent_encryption").unwrap();
    let run = wait_for_completion(&state);
    assert_eq!(run.status, "CONTAINED_RECOVERED", "{run:?}");
    assert!(run.recovered);
    assert_eq!(run.pre_tree_blake3, run.post_tree_blake3);
}

#[test]
fn multiworker_evidence_is_aggregated_under_one_ragid() {
    let temp = tempfile::tempdir().unwrap();
    let state = AppState::new(temp.path()).unwrap();
    state.start_simulation("multi_process").unwrap();
    let run = wait_for_completion(&state);
    assert_eq!(run.status, "CONTAINED_RECOVERED", "{run:?}");
    let telemetry = state.telemetry().unwrap();
    let process = telemetry
        .processes
        .iter()
        .find(|process| process.ragid == run.ragid)
        .unwrap();
    assert!(process.worker_pids.len() > 1);
    assert_eq!(process.status, "RECOVERED");
}

#[test]
fn canary_access_contains_after_one_file() {
    let temp = tempfile::tempdir().unwrap();
    let state = AppState::new(temp.path()).unwrap();
    state.start_simulation("canary").unwrap();
    let run = wait_for_completion(&state);
    assert_eq!(run.status, "CONTAINED_RECOVERED", "{run:?}");
    assert_eq!(run.touched_files, 1);
    assert!(run.recovered);
}

#[test]
fn benign_gzip_completes_without_containment() {
    let temp = tempfile::tempdir().unwrap();
    let state = AppState::new(temp.path()).unwrap();
    state.start_simulation("compression").unwrap();
    let run = wait_for_completion(&state);
    assert_eq!(run.status, "COMPLETED_RECOVERED", "{run:?}");
    assert!(!run.contained);
    assert!(run.recovered);
    assert_eq!(run.pre_tree_blake3, run.post_tree_blake3);
}
