//! `tool analyze -i <$MFT> [--csv <out>] [-o <report.html>] [--baseline <file>] [--rules <dir>]`
//! and `tool baseline build`.

use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;
use baseline::Baseline;
use clap::{ArgGroup, Parser, Subcommand};
use report::{CsvWriter, HtmlReport, Input, Provenance};
use sigma::Rules;

#[derive(Debug, Parser)]
#[command(version, about = "NTFS triage from $MFT")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Parse a raw $MFT and write an HTML report and/or one CSV row per record.
    #[command(group(ArgGroup::new("out").args(["csv", "output"]).required(true).multiple(true)))]
    Analyze {
        /// Raw $MFT file.
        #[arg(short, long)]
        input: PathBuf,
        /// CSV output path.
        #[arg(long)]
        csv: Option<PathBuf>,
        /// HTML report output path.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Baseline file from `tool baseline build`; without it the `baseline` column is empty.
        #[arg(long)]
        baseline: Option<PathBuf>,
        /// Directory of Sigma rules (`.yml`/`.yaml`); without it no rules run.
        #[arg(long)]
        rules: Option<PathBuf>,
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
            output,
            baseline,
            rules,
        } => analyze(
            &input,
            csv.as_deref(),
            output.as_deref(),
            baseline.as_deref(),
            rules.as_deref(),
        ),
        Command::Baseline(BaselineCommand::Build { vwr, o }) => build_baseline(&vwr, &o),
    }
}

fn analyze(
    input: &Path,
    csv: Option<&Path>,
    output: Option<&Path>,
    baseline: Option<&Path>,
    rules: Option<&Path>,
) -> anyhow::Result<()> {
    let rules = rules
        .map(|dir| {
            Rules::load(dir).with_context(|| format!("loading rules from {}", dir.display()))
        })
        .transpose()?;
    let baseline_name = baseline.map(file_name);
    let baseline = baseline
        .map(|path| {
            let file =
                std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
            Baseline::load(file).with_context(|| format!("loading {}", path.display()))
        })
        .transpose()?;
    let mft = File::open(input).with_context(|| format!("opening {}", input.display()))?;
    let mut csv_writer = csv
        .map(|path| {
            let file =
                File::create(path).with_context(|| format!("creating {}", path.display()))?;
            Ok::<_, anyhow::Error>(CsvWriter::new(BufWriter::new(file))?)
        })
        .transpose()?;
    let html_file = output
        .map(|path| File::create(path).with_context(|| format!("creating {}", path.display())))
        .transpose()?;
    // Paths need every parent, so all records are read before the first row is written.
    let records = mft_parse::records(BufReader::new(mft))?;
    let record_size = records.record_size();
    let mut html = html_file.map(|file| {
        let input = Input {
            name: file_name(input),
            record_size,
            baseline: baseline_name,
            rules: rules.as_ref().map_or(0, Rules::count),
        };
        (file, HtmlReport::new(PROVENANCE, input))
    });
    let entries = mft_parse::merge_extensions(records.collect::<Result<Vec<_>, _>>()?, record_size);
    for row in analyze::rows(&entries, baseline.as_ref(), rules.as_ref()) {
        if let Some(writer) = &mut csv_writer {
            writer.write(&row)?;
        }
        if let Some((_, report)) = &mut html {
            report.add(&row);
        }
    }
    if let Some(writer) = csv_writer {
        writer.finish()?.flush()?;
    }
    if let Some((file, report)) = html {
        report.finish(BufWriter::new(file))?.flush()?;
    }
    Ok(())
}

const PROVENANCE: Provenance = Provenance {
    version: env!("CARGO_PKG_VERSION"),
    commit: env!("TOOL_COMMIT"),
    source_url: env!("CARGO_PKG_REPOSITORY"),
};

/// The file name alone: the report must not reveal the analyst's directories.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn build_baseline(vwr: &Path, out: &Path) -> anyhow::Result<()> {
    let csv = File::open(vwr).with_context(|| format!("opening {}", vwr.display()))?;
    let file = File::create(out).with_context(|| format!("creating {}", out.display()))?;
    let mut writer = BufWriter::new(file);
    baseline::build(BufReader::new(csv), &mut writer)?;
    writer.flush()?;
    Ok(())
}
