mod model;
mod replay;
use anyhow::{Context, Result};
use clap::Parser;
use std::{
    fs::File,
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
};

#[derive(Parser)]
#[command(about = "Read-only offline classifier/scorer replay; no monitoring or process signaling")]
struct Args {
    /// JSON file containing experimental scoring likelihoods.
    #[arg(long)]
    config: PathBuf,
    /// JSON Lines events; input paths are relative to the events file directory.
    #[arg(long)]
    events: PathBuf,
    /// Create a new JSON report; never overwrite an existing path. Omit for stdout.
    #[arg(long)]
    output: Option<PathBuf>,
}
fn main() -> Result<()> {
    let args = Args::parse();
    let config: model::ReplayConfig =
        serde_json::from_reader(File::open(&args.config).context("open config")?)
            .context("parse config")?;
    let event_path = args.events.canonicalize().context("resolve events")?;
    let root = event_path.parent().context("events has no parent")?;
    let mut events = Vec::new();
    let mut reader = BufReader::new(File::open(&event_path)?);
    let mut index = 0usize;
    loop {
        // Limit allocation before finding the newline, not after reading a
        // potentially unbounded line from an untrusted file.
        let mut bytes = Vec::new();
        let n = Read::by_ref(&mut reader)
            .take(16_386)
            .read_until(b'\n', &mut bytes)?;
        if n == 0 {
            break;
        }
        index += 1;
        if bytes.last() == Some(&b'\n') {
            bytes.pop();
        }
        if bytes.len() > 16_384 {
            anyhow::bail!("event line {} exceeds 16 KiB", index);
        }
        let line =
            std::str::from_utf8(&bytes).with_context(|| format!("decode event line {}", index))?;
        if line.trim().is_empty() {
            continue;
        }
        if events.len() >= 10_000 {
            anyhow::bail!("maximum 10,000 events per replay");
        }
        let event =
            serde_json::from_str(line).with_context(|| format!("parse event line {}", index))?;
        events.push(event);
    }
    if events.is_empty() {
        anyhow::bail!("no events");
    }
    let report = replay::run(config, &events, root)?;
    let json = serde_json::to_vec_pretty(&report)?;
    if let Some(path) = args.output {
        // create_new rejects existing files, hard links and symlinks. There is no
        // need for the user to redirect stdout to a potentially existing input.
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .with_context(|| format!("create new report {}", path.display()))?;
        file.write_all(&json)?;
        file.write_all(b"\n")?;
        eprintln!("Wrote {}", path.display());
    } else {
        println!("{}", String::from_utf8(json)?);
    }
    Ok(())
}
