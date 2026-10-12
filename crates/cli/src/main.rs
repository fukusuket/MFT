//! `tool analyze -i <$MFT> [--usn <$J>] [--csv <out>] [-o <report.html>] [--jsonl <out>] [--baseline <file>] [--rules <dir>]...`
//! and `tool baseline build`.

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;
use baseline::Baseline;
use clap::{ArgGroup, Parser, Subcommand};
use ntfs_types::Filetime;
use report::{CsvWriter, HtmlReport, Input, JsonlWriter, Provenance, WindowText};
use sigma::Rules;

#[derive(Debug, Parser)]
#[command(version, about = "NTFS triage from $MFT")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
// This value is created once at startup; boxing individual clap fields would only add allocations.
#[allow(clippy::large_enum_variant)]
enum Command {
    /// Parse a raw $MFT and write an HTML report and/or one CSV row per record.
    #[command(group(ArgGroup::new("out").args(["csv", "output", "jsonl"]).required(true).multiple(true)))]
    Analyze {
        /// Raw $MFT file.
        #[arg(short, long)]
        input: PathBuf,
        /// Raw $UsnJrnl:$J file; adds one CSV/JSONL row per USN record after the $MFT rows.
        #[arg(long)]
        usn: Option<PathBuf>,
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
        /// Directory of Sigma rules (`.yml`/`.yaml`, subdirectories included); may be repeated.
        /// Without it no rules run.
        #[arg(long)]
        rules: Vec<PathBuf>,
        /// Window start, `YYYY-MM-DD` or `YYYY-MM-DDTHH:MM:SS` (inclusive). The HTML report lists
        /// only rows in the window; CSV/JSONL mark them with `in_window`.
        #[arg(long)]
        from: Option<String>,
        /// Window end, `YYYY-MM-DD` (the whole day) or `YYYY-MM-DDTHH:MM:SS` (the whole second).
        #[arg(long)]
        to: Option<String>,
        /// UTC offset of `--from` and `--to`, `±HH:MM`; UTC when omitted. Output times stay UTC.
        #[arg(long, allow_hyphen_values = true)]
        tz: Option<String>,
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
            usn,
            csv,
            output,
            jsonl,
            baseline,
            rules,
            from,
            to,
            tz,
        } => analyze(
            &input,
            usn.as_deref(),
            (csv.as_deref(), output.as_deref(), jsonl.as_deref()),
            baseline.as_deref(),
            &rules,
            (from.as_deref(), to.as_deref(), tz.as_deref()),
        ),
        Command::Baseline(BaselineCommand::Build { vwr, o }) => build_baseline(&vwr, &o),
    }
}

fn analyze(
    input: &Path,
    usn: Option<&Path>,
    (csv, output, jsonl): (Option<&Path>, Option<&Path>, Option<&Path>),
    baseline: Option<&Path>,
    rules: &[PathBuf],
    (from, to, tz): (Option<&str>, Option<&str>, Option<&str>),
) -> anyhow::Result<()> {
    let window = window(from, to, tz)?;
    // Skipped rules are listed beside the CSV and JSONL outputs, or beside the HTML report when
    // it is the only output.
    let listed = |path: Option<&Path>, extension| {
        path.filter(|_| !rules.is_empty())
            .map(|path| skipped_path(path, extension))
    };
    let skipped_csv = listed(csv.or(output.filter(|_| jsonl.is_none())), "csv");
    let skipped_jsonl = listed(jsonl, "jsonl");
    let outputs = [
        (csv, "CSV output"),
        (output, "HTML output"),
        (jsonl, "JSONL output"),
    ];
    for (n, &(path, name)) in outputs.iter().enumerate() {
        refuse_same_file(input, path, "input", name)?;
        if let Some(usn) = usn {
            refuse_same_file(usn, path, "USN input", name)?;
        }
        let Some(path) = path else { continue };
        for &(later, later_name) in &outputs[n + 1..] {
            refuse_same_file(path, later, name, later_name)?;
        }
    }
    let name = "skipped-rules list";
    for list in [&skipped_csv, &skipped_jsonl].into_iter().flatten() {
        refuse_same_file(input, Some(list), "input", name)?;
        if let Some(usn) = usn {
            refuse_same_file(usn, Some(list), "USN input", name)?;
        }
        for &(path, output_name) in &outputs {
            refuse_same_file(list, path, name, output_name)?;
        }
    }
    let rules = if rules.is_empty() {
        None
    } else {
        let dirs: Vec<&Path> = rules.iter().map(PathBuf::as_path).collect();
        Some(Rules::load(&dirs, &detect::EMITTED).context("loading rules")?)
    };
    let baseline_name = baseline.map(file_name);
    let baseline = baseline
        .map(|path| {
            let file =
                std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
            Baseline::load(file).with_context(|| format!("loading {}", path.display()))
        })
        .transpose()?;
    let mft = File::open(input).with_context(|| format!("opening {}", input.display()))?;
    let journal = usn
        .map(|path| File::open(path).with_context(|| format!("opening {}", path.display())))
        .transpose()?;
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
            skipped_rules: rules.as_ref().map_or(0, |r| r.skipped().len()),
            window: window.map(|window| WindowText {
                from: from.map(str::to_string),
                to: to.map(str::to_string),
                tz: tz.unwrap_or("+00:00").to_string(),
                start: window.start,
            }),
        };
        (pending, file, HtmlReport::new(PROVENANCE, input))
    });
    let entries = mft_parse::merge_extensions(records.collect::<Result<Vec<_>, _>>()?, record_size);
    // Read before the $MFT rows, which tell whether the journal has each file too.
    let journal = journal
        .map(|journal| {
            usn_parse::records(BufReader::new(journal))
                .collect::<Result<Vec<_>, _>>()
                .context("reading the USN journal")
        })
        .transpose()?;
    let in_journal = journal.as_deref().unwrap_or_default();
    for row in analyze::rows(
        &entries,
        in_journal,
        baseline.as_ref(),
        rules.as_ref(),
        window.as_ref(),
    ) {
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
    if let (Some((_, _, report)), Some(baseline)) = (&mut html, &baseline) {
        report.check_edition(baseline);
    }
    if let Some(journal) = &journal {
        // After the $MFT rows, in $J order; the HTML report shows only their findings.
        for row in analyze::usn_rows(
            &entries,
            journal,
            baseline.as_ref(),
            rules.as_ref(),
            window.as_ref(),
        ) {
            if let Some((_, writer)) = &mut csv_writer {
                writer.write_usn(&row)?;
            }
            if let Some((_, writer)) = &mut jsonl_writer {
                writer.write_usn(&row)?;
            }
            if let Some((_, _, report)) = &mut html {
                report.add_usn(&row);
            }
        }
    }
    let mut skipped = Vec::new();
    if let Some(rules) = &rules {
        if let Some(path) = &skipped_csv {
            let (pending, file) = AtomicOutput::create(path)?;
            report::write_skipped_csv(BufWriter::new(file), rules.skipped())?.flush()?;
            skipped.push(pending);
        }
        if let Some(path) = &skipped_jsonl {
            let (pending, file) = AtomicOutput::create(path)?;
            report::write_skipped_jsonl(BufWriter::new(file), rules.skipped())?.flush()?;
            skipped.push(pending);
        }
    }
    // The lists first: a report is never left without the list of rules it did not run.
    for pending in skipped {
        pending.commit()?;
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

/// The window `--from`, `--to` and `--tz` select, in UTC; `None` without `--from` or `--to`.
fn window(
    from: Option<&str>,
    to: Option<&str>,
    tz: Option<&str>,
) -> anyhow::Result<Option<analyze::Window>> {
    let offset = tz.map(parse_offset).transpose()?.unwrap_or(0);
    let utc = |local: u64, flag: &str| {
        local
            .checked_add_signed(-offset)
            .map(Filetime::from_raw)
            .with_context(|| format!("{flag} with --tz is before 1601 in UTC"))
    };
    if from.is_none() && to.is_none() {
        return Ok(None);
    }
    let start = from
        .map(|text| utc(parse_time(text, "--from")?.0.raw(), "--from"))
        .transpose()?;
    let end = to
        .map(|text| {
            let (time, length) = parse_time(text, "--to")?;
            utc(time.raw().saturating_add(length), "--to")
        })
        .transpose()?;
    if let (Some(start), Some(end)) = (start, end)
        && start >= end
    {
        anyhow::bail!("--from is after --to: the window holds no time");
    }
    Ok(Some(analyze::Window { start, end }))
}

/// `YYYY-MM-DD` or `YYYY-MM-DDTHH:MM:SS` in UTC, and its length in 100 ns ticks (a day or a
/// second).
fn parse_time(text: &str, flag: &str) -> anyhow::Result<(Filetime, u64)> {
    // Three fields of exactly `widths` digits each.
    let fields = |text: &str, separator: char, widths: [usize; 3]| -> Option<(u64, u64, u64)> {
        let parts: Vec<&str> = text.split(separator).collect();
        let [a, b, c] = parts.as_slice() else {
            return None;
        };
        let field = |part: &str, width: usize| {
            (part.len() == width && part.bytes().all(|b| b.is_ascii_digit()))
                .then(|| part.parse::<u64>().ok())
                .flatten()
        };
        Some((
            field(a, widths[0])?,
            field(b, widths[1])?,
            field(c, widths[2])?,
        ))
    };
    let date = |text| fields(text, '-', [4, 2, 2]);
    let parsed = match text.split_once('T') {
        None => date(text).map(|date| (date, (0, 0, 0), 86_400)),
        Some((d, time)) => date(d)
            .zip(fields(time, ':', [2, 2, 2]))
            .map(|(d, t)| (d, t, 1)),
    };
    parsed
        .and_then(|(date, time, seconds)| {
            Some((Filetime::from_utc(date, time)?, seconds * 10_000_000))
        })
        .with_context(|| {
            format!(
                "{flag} {text:?}: expected a date YYYY-MM-DD or YYYY-MM-DDTHH:MM:SS that exists"
            )
        })
}

/// `±HH:MM` as 100 ns ticks east of UTC.
fn parse_offset(text: &str) -> anyhow::Result<i64> {
    let parse = || {
        let (sign, rest) = match text.as_bytes().first()? {
            b'+' => (1, &text[1..]),
            b'-' => (-1, &text[1..]),
            _ => return None,
        };
        let (h, m) = rest.split_once(':')?;
        let two_digits = |s: &str| {
            (s.len() == 2 && s.bytes().all(|b| b.is_ascii_digit()))
                .then(|| s.parse::<i64>().ok())
                .flatten()
        };
        let (h, m) = (two_digits(h)?, two_digits(m)?);
        (h <= 23 && m <= 59).then(|| sign * (h * 3600 + m * 60) * 10_000_000)
    };
    parse()
        .with_context(|| format!("--tz {text:?}: expected a UTC offset such as +09:00 or -05:00"))
}

/// `out.csv` → `out.skipped-rules.csv`: the list of rules that were not loaded, beside `output`.
fn skipped_path(output: &Path, extension: &str) -> PathBuf {
    let mut name = output.file_stem().unwrap_or_default().to_os_string();
    name.push(format!(".skipped-rules.{extension}"));
    output.with_file_name(name)
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
