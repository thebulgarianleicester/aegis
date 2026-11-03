use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
#[test]
fn paired_manifest_and_alias_guard() {
    let dir = std::env::temp_dir().join(format!(
        "aegis-test-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&dir).unwrap();
    let data = vec![0u8; 4096];
    fs::write(dir.join("pre"), &data).unwrap();
    fs::write(dir.join("post"), &data).unwrap();
    fs::write(
        dir.join("manifest.csv"),
        "pre,post,label\npre,post,benign\n",
    )
    .unwrap();
    let exe = env!("CARGO_BIN_EXE_aegis-bench");
    let success = Command::new(exe)
        .args([
            "--manifest",
            dir.join("manifest.csv").to_str().unwrap(),
            "--report",
            dir.join("report.json").to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(success.success());
    let result: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join("report.json")).unwrap()).unwrap();
    assert_eq!(result["confusion"]["true_negative"], 1);
    fs::hard_link(dir.join("pre"), dir.join("alias")).unwrap();
    let guarded = Command::new(exe)
        .args([
            "--manifest",
            dir.join("manifest.csv").to_str().unwrap(),
            "--report",
            dir.join("alias").to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(!guarded.success());
    assert_eq!(fs::read(dir.join("pre")).unwrap(), data);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn explicit_intermittent_labels_and_workload_summaries() {
    let dir = std::env::temp_dir().join(format!(
        "aegis-label-test-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("pre"), vec![0u8; 8192]).unwrap();
    fs::write(dir.join("post"), vec![0u8; 8192]).unwrap();
    let manifest = dir.join("manifest.csv");
    let output = dir.join("report.json");
    let exe = env!("CARGO_BIN_EXE_aegis-bench");
    fs::write(
        &manifest,
        "pre,post,label,positive_blocks,workload\npre,post,suspicious,1,striped\n",
    )
    .unwrap();
    let status = Command::new(exe)
        .arg("--manifest")
        .arg(&manifest)
        .arg("--report")
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());
    let result: serde_json::Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    assert_eq!(result["observations"][0]["label"], "benign");
    assert_eq!(result["observations"][1]["label"], "suspicious");
    assert_eq!(
        result["workloads"]["striped"]["demo_heuristic"]["true_negative"],
        1
    );
    assert_eq!(
        result["workloads"]["striped"]["demo_heuristic"]["false_negative"],
        1
    );
    fs::write(
        &manifest,
        "pre,post,label,positive_blocks,workload\npre,post,suspicious,2,striped\n",
    )
    .unwrap();
    let status = Command::new(exe)
        .arg("--manifest")
        .arg(&manifest)
        .arg("--report")
        .arg(&output)
        .status()
        .unwrap();
    assert!(!status.success());
    fs::remove_dir_all(dir).unwrap();
}
