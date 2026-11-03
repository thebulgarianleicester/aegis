use aegis_daemon::{http, AppState};
use anyhow::{Context, Result};
use clap::Parser;
use std::{net::TcpListener, path::PathBuf};

#[derive(Debug, Parser)]
#[command(about = "AegisFS isolated-lab API and live telemetry daemon")]
struct Args {
    #[arg(long, default_value = "0.0.0.0:8787")]
    bind: String,
    #[arg(long, default_value = "/tmp/aegisfs-lab")]
    lab_root: PathBuf,
    /// Optional production React build directory (web/dist).
    #[arg(long)]
    web_root: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let state = AppState::new(&args.lab_root)?;
    let listener =
        TcpListener::bind(&args.bind).with_context(|| format!("bind lab API to {}", args.bind))?;
    println!("AegisFS isolated-lab API listening on http://{}", args.bind);
    println!("Lab data: {}", args.lab_root.display());
    println!("WARNING: no host filesystem protection or host process containment is active.");
    http::serve(listener, state, args.web_root)
}
