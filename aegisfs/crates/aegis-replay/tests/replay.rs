use std::{fs, process::Command};

const CONFIG: &str = r#"{"uniform":{"benign":0.2,"suspicious":0.8},"header_change":{"benign":0.1,"suspicious":0.5},"unlink_or_rename":{"benign":0.2,"suspicious":0.8},"directory_jump":{"benign":0.2,"suspicious":0.8},"canary":{"benign":0.2,"suspicious":0.8},"decay_per_second":0.5,"threshold":2.0,"max_groups":2}"#;
#[test]
fn replay_is_read_only_and_evidence_is_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    let data = vec![0u8; 4096];
    fs::write(p.join("pre"), &data).unwrap();
    fs::write(p.join("post"), &data).unwrap();
    fs::write(p.join("config.json"), CONFIG).unwrap();
    fs::write(p.join("events.jsonl"),"{\"group\":\"worker\",\"timestamp_ns\":1,\"pre\":\"pre\",\"post\":\"post\",\"offset\":0,\"length\":4096}\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_aegis-replay"))
        .arg("--config")
        .arg(p.join("config.json"))
        .arg("--events")
        .arg(p.join("events.jsonl"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let r: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(r["schema"], "aegisfs-replay-v1");
    assert_eq!(r["events"][0]["evidence"]["uniform"], false);
    assert!(r["events"][0]["evidence"]["header_change"].is_null());
    assert!(r["events"][0]["evidence"]["canary"].is_null());
    assert_eq!(r["events"][0]["score"], 0.0);
    assert_eq!(fs::read(p.join("pre")).unwrap(), data);
    assert_eq!(fs::read(p.join("post")).unwrap(), data);
}
#[test]
fn invalid_order_fails_without_partial_report() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    fs::write(p.join("data"), vec![0; 4096]).unwrap();
    fs::write(p.join("config.json"), CONFIG).unwrap();
    let event = |time| {
        format!("{{\"group\":\"g\",\"timestamp_ns\":{time},\"pre\":\"data\",\"post\":\"data\",\"offset\":0,\"length\":4096}}\n")
    };
    fs::write(p.join("events"), event(2) + &event(1)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_aegis-replay"))
        .arg("--config")
        .arg(p.join("config.json"))
        .arg("--events")
        .arg(p.join("events"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn output_path_must_be_new_and_cannot_overwrite_input() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    fs::write(p.join("data"), vec![0; 4096]).unwrap();
    fs::write(p.join("config.json"), CONFIG).unwrap();
    let events="{\"group\":\"g\",\"timestamp_ns\":1,\"pre\":\"data\",\"post\":\"data\",\"offset\":0,\"length\":4096}\n";
    fs::write(p.join("events"), events).unwrap();
    let exe = env!("CARGO_BIN_EXE_aegis-replay");
    let run = |out: &std::path::Path| {
        Command::new(exe)
            .arg("--config")
            .arg(p.join("config.json"))
            .arg("--events")
            .arg(p.join("events"))
            .arg("--output")
            .arg(out)
            .output()
            .unwrap()
    };
    assert!(!run(&p.join("data")).status.success());
    assert_eq!(fs::read(p.join("data")).unwrap(), vec![0; 4096]);
    assert!(run(&p.join("report.json")).status.success());
    assert!(!run(&p.join("report.json")).status.success());
}

#[test]
fn oversize_event_line_is_rejected_without_report() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    fs::write(p.join("config.json"), CONFIG).unwrap();
    fs::write(p.join("events"), vec![b'x'; 17000]).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_aegis-replay"))
        .arg("--config")
        .arg(p.join("config.json"))
        .arg("--events")
        .arg(p.join("events"))
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("exceeds 16 KiB"));
}

#[test]
fn header_evidence_and_relative_time_are_wired_correctly() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    let mut before = vec![0u8; 4096];
    before[..5].copy_from_slice(b"%PDF-");
    let after: Vec<u8> = (0..4096).map(|i| (i % 256) as u8).collect();
    fs::write(p.join("pre"), before).unwrap();
    fs::write(p.join("post"), after).unwrap();
    fs::write(p.join("config"), CONFIG).unwrap();
    let line = |ts: u64| {
        format!("{{\"group\":\"g\",\"timestamp_ns\":{ts},\"pre\":\"pre\",\"post\":\"post\",\"offset\":0,\"length\":4096}}\n")
    };
    let epoch = 9_000_000_000_000_000_000u64;
    fs::write(p.join("events"), line(epoch) + &line(epoch + 1_000_000_000)).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_aegis-replay"))
        .arg("--config")
        .arg(p.join("config"))
        .arg("--events")
        .arg(p.join("events"))
        .output()
        .unwrap();
    assert!(out.status.success());
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["events"][0]["evidence"]["header_change"], true);
    assert_eq!(report["events"][0]["evidence"]["uniform"], false);
    assert!((report["events"][0]["score"].as_f64().unwrap() - 1.25f64.ln()).abs() < 1e-12);
    assert_eq!(report["events"][1]["elapsed_seconds"], 1.0);
}
