//! `tool analyze -i <$MFT> --csv <out>`.

use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Parser, Subcommand};
use report::CsvWriter;

#[derive(Debug, Parser)]
#[command(version, about = "NTFS triage from $MFT")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Parse a raw $MFT and write one CSV row per record.
    Analyze {
        /// Raw $MFT file.
        #[arg(short, long)]
        input: PathBuf,
        /// CSV output path.
        #[arg(long)]
        csv: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let Cli { command } = Cli::parse();
    let Command::Analyze { input, csv } = command;
    analyze(&input, &csv)
}

fn analyze(input: &Path, csv: &Path) -> anyhow::Result<()> {
    let mft = File::open(input).with_context(|| format!("opening {}", input.display()))?;
    let out = File::create(csv).with_context(|| format!("creating {}", csv.display()))?;
    let mut writer = CsvWriter::new(BufWriter::new(out))?;
    // Paths need every parent, so all records are read before the first row is written.
    let entries = mft_parse::records(BufReader::new(mft))?.collect::<Result<Vec<_>, _>>()?;
    for row in analyze::rows(&entries) {
        writer.write(&row)?;
    }
    writer.finish()?.flush()?;
    Ok(())
}
