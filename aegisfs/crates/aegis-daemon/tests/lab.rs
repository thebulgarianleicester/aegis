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
