use aegis_journal::TxJournal;
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(about = "Inspect or recover an isolated AegisFS transaction group")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the group's reverse-ordered transaction ledger.
    Ledger {
        #[command(flatten)]
        target: Target,
    },
    /// Conflict-check and reverse replay all unresolved group operations.
    Rollback {
        #[command(flatten)]
        target: Target,
    },
}

#[derive(Debug, clap::Args)]
struct Target {
    #[arg(long)]
    data_root: PathBuf,
    #[arg(long)]
    journal_root: PathBuf,
    #[arg(long)]
    ragid: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    match args.command {
        Command::Ledger { target } => {
            let journal = open(target)?;
            println!("{}", serde_json::to_string_pretty(&journal.ledger()?)?);
        }
        Command::Rollback { target } => {
            let mut journal = open(target)?;
            println!("{}", serde_json::to_string_pretty(&journal.rollback()?)?);
        }
    }
    Ok(())
}

fn open(target: Target) -> Result<TxJournal> {
    TxJournal::open(&target.data_root, &target.journal_root, &target.ragid)
}
