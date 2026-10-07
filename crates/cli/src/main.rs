//! `tool analyze -i <$MFT> --csv <out> [--baseline <file>]` and `tool baseline build`.

use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;
use baseline::Baseline;
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
        /// Baseline file from `tool baseline build`; without it the `baseline` column is empty.
        #[arg(long)]
        baseline: Option<PathBuf>,
    },
    /// Windows baselines.
    #[command(subcommand)]
    Baseline(BaselineCommand),
}

#[derive(Debug, Subcommand)]
enum BaselineCommand {
    /// Build a baseline file from a VanillaWindowsReference CSV.
    Build {
        /// VanillaWindowsReference CSV.
        #[arg(long)]
        vwr: PathBuf,
        /// Output baseline file.
        #[arg(short)]
        o: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let Cli { command } = Cli::parse();
    match command {
        Command::Analyze {
            input,
            csv,
            baseline,
        } => analyze(&input, &csv, baseline.as_deref()),
        Command::Baseline(BaselineCommand::Build { vwr, o }) => build_baseline(&vwr, &o),
    }
}

fn analyze(input: &Path, csv: &Path, baseline: Option<&Path>) -> anyhow::Result<()> {
    let baseline = baseline
        .map(|path| {
            let file =
                std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
            Baseline::load(file).with_context(|| format!("loading {}", path.display()))
        })
        .transpose()?;
    let mft = File::open(input).with_context(|| format!("opening {}", input.display()))?;
    let out = File::create(csv).with_context(|| format!("creating {}", csv.display()))?;
    let mut writer = CsvWriter::new(BufWriter::new(out))?;
    // Paths need every parent, so all records are read before the first row is written.
    let entries = mft_parse::records(BufReader::new(mft))?.collect::<Result<Vec<_>, _>>()?;
    for row in analyze::rows(&entries, baseline.as_ref()) {
        writer.write(&row)?;
    }
    writer.finish()?.flush()?;
    Ok(())
}

fn build_baseline(vwr: &Path, out: &Path) -> anyhow::Result<()> {
    let csv = File::open(vwr).with_context(|| format!("opening {}", vwr.display()))?;
    let file = File::create(out).with_context(|| format!("creating {}", out.display()))?;
    let mut writer = BufWriter::new(file);
    baseline::build(BufReader::new(csv), &mut writer)?;
    writer.flush()?;
    Ok(())
}
