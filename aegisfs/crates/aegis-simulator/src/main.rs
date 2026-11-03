use aegis_simulator::{run, RunConfig, SimEvent, SimulationControl, Workload};
use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(about = "Run a safe workload in new, empty AegisFS lab directories")]
struct Args {
    #[arg(long)]
    data_root: PathBuf,
    #[arg(long)]
    journal_root: PathBuf,
    #[arg(long, default_value = "intermittent")]
    workload: String,
    #[arg(long, default_value_t = 12)]
    files: usize,
    #[arg(long, default_value = "cli-lab")]
    ragid: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let workload: Workload = args.workload.parse()?;
    let summary = run(
        RunConfig {
            workload,
            data_root: args.data_root,
            journal_root: args.journal_root,
            ragid: args.ragid,
            root_pid: std::process::id(),
            files: args.files,
        },
        |event| {
            if !matches!(event, SimEvent::Finished(_)) {
                eprintln!("{}", serde_json::to_string(&event).unwrap_or_default());
            }
            SimulationControl::Continue
        },
    )?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}
