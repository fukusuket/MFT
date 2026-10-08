//! `tool analyze -i <$MFT> [--csv <out>] [-o <report.html>] [--jsonl <out>] [--baseline <file>] [--rules <dir>]`
//! and `tool baseline build`.

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;
use baseline::Baseline;
use clap::{ArgGroup, Parser, Subcommand};
use report::{CsvWriter, HtmlReport, Input, JsonlWriter, Provenance};
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
    #[command(group(ArgGroup::new("out").args(["csv", "output", "jsonl"]).required(true).multiple(true)))]
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
        /// JSON Lines output path: one object per record, the CSV columns as typed fields.
        #[arg(long)]
        jsonl: Option<PathBuf>,
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
            jsonl,
            baseline,
            rules,
        } => analyze(
            &input,
            csv.as_deref(),
            output.as_deref(),
            jsonl.as_deref(),
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
    jsonl: Option<&Path>,
    baseline: Option<&Path>,
    rules: Option<&Path>,
) -> anyhow::Result<()> {
    let outputs = [
        (csv, "CSV output"),
        (output, "HTML output"),
        (jsonl, "JSONL output"),
    ];
    for (n, &(path, name)) in outputs.iter().enumerate() {
        refuse_same_file(input, path, "input", name)?;
        let Some(path) = path else { continue };
        for &(later, later_name) in &outputs[n + 1..] {
            refuse_same_file(path, later, name, later_name)?;
        }
    }
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
            let (pending, file) = AtomicOutput::create(path)?;
            Ok::<_, anyhow::Error>((pending, CsvWriter::new(BufWriter::new(file))?))
        })
        .transpose()?;
    let mut jsonl_writer = jsonl
        .map(|path| {
            let (pending, file) = AtomicOutput::create(path)?;
            Ok::<_, anyhow::Error>((pending, JsonlWriter::new(BufWriter::new(file))))
        })
        .transpose()?;
    let html_file = output.map(AtomicOutput::create).transpose()?;
    // Paths need every parent, so all records are read before the first row is written.
    let records = mft_parse::records(BufReader::new(mft))?;
    let record_size = records.record_size();
    let mut html = html_file.map(|(pending, file)| {
        let input = Input {
            name: file_name(input),
            record_size,
            baseline: baseline_name,
            rules: rules.as_ref().map_or(0, Rules::count),
        };
        (pending, file, HtmlReport::new(PROVENANCE, input))
    });
    let entries = mft_parse::merge_extensions(records.collect::<Result<Vec<_>, _>>()?, record_size);
    for row in analyze::rows(&entries, baseline.as_ref(), rules.as_ref()) {
        if let Some((_, writer)) = &mut csv_writer {
            writer.write(&row)?;
        }
        if let Some((_, writer)) = &mut jsonl_writer {
            writer.write(&row)?;
        }
        if let Some((_, _, report)) = &mut html {
            report.add(&row);
        }
    }
    if let Some((pending, writer)) = jsonl_writer {
        writer.finish()?.flush()?;
        pending.commit()?;
    }
    if let Some((pending, writer)) = csv_writer {
        writer.finish()?.flush()?;
        pending.commit()?;
    }
    if let Some((pending, file, report)) = html {
        report.finish(BufWriter::new(file))?.flush()?;
        pending.commit()?;
    }
    Ok(())
}

#[derive(Debug)]
struct AtomicOutput {
    destination: PathBuf,
    temporary: PathBuf,
    committed: bool,
}

impl AtomicOutput {
    fn create(destination: &Path) -> anyhow::Result<(Self, File)> {
        let parent = destination.parent().unwrap_or_else(|| Path::new("."));
        let name = destination.file_name().unwrap_or_default();
        let mut temporary_name = OsString::from(".");
        temporary_name.push(name);
        temporary_name.push(format!(".tool-tmp-{}", std::process::id()));
        let temporary = parent.join(temporary_name);
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .with_context(|| format!("creating {}", destination.display()))?;
        Ok((
            Self {
                destination: destination.to_path_buf(),
                temporary,
                committed: false,
            },
            file,
        ))
    }

    fn commit(mut self) -> anyhow::Result<()> {
        std::fs::rename(&self.temporary, &self.destination)
            .with_context(|| format!("replacing {}", self.destination.display()))?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for AtomicOutput {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.temporary);
        }
    }
}

fn refuse_same_file(
    input: &Path,
    output: Option<&Path>,
    input_name: &str,
    output_name: &str,
) -> anyhow::Result<()> {
    let same = output.is_some_and(|output| {
        output == input
            || std::fs::canonicalize(input)
                .ok()
                .zip(std::fs::canonicalize(output).ok())
                .is_some_and(|(input, output)| input == output)
    });
    if same {
        anyhow::bail!(
            "{input_name} and {output_name} refer to the same file: {}",
            input.display()
        );
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
    refuse_same_file(vwr, Some(out), "VWR input", "baseline output")?;
    let csv = File::open(vwr).with_context(|| format!("opening {}", vwr.display()))?;
    let (pending, file) = AtomicOutput::create(out)?;
    let mut writer = BufWriter::new(file);
    baseline::build(BufReader::new(csv), &mut writer)?;
    writer.flush()?;
    pending.commit()?;
    Ok(())
}
