//! Report writers. Phase 1: CSV, one row per `$MFT` record.

use std::fmt::Write as _;
use std::io::Write;

use analyze::{Row, UsnRow};
use baseline::Status;
use mft_parse::DiagCode;
use ntfs_types::Filetime;
use resolve::Resolution;
use sigma::{Finding, Level};

mod base64;
mod html;
mod jsonl;

pub use html::{HtmlReport, Input, Provenance};
pub use jsonl::JsonlWriter;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("writing CSV: {0}")]
    Csv(#[from] csv::Error),
    #[error("writing HTML: {0}")]
    Html(#[from] std::io::Error),
    #[error("writing JSONL: {0}")]
    Json(#[from] serde_json::Error),
}

const HEADER: [&str; 15] = [
    "entry",
    "sequence",
    "in_use",
    "name",
    "path",
    "path_state",
    "baseline",
    "findings",
    "si_created",
    "fn_created",
    "diagnostics",
    "source",
    "usn",
    "reasons",
    "event_time",
];

/// CSV with one row per `$MFT` record; columns as in `HEADER`.
#[derive(Debug)]
pub struct CsvWriter<W: Write> {
    csv: csv::Writer<W>,
}

impl<W: Write> CsvWriter<W> {
    pub fn new(out: W) -> Result<Self, Error> {
        let mut csv = csv::Writer::from_writer(out);
        csv.write_record(HEADER)?;
        Ok(Self { csv })
    }

    pub fn write(&mut self, row: &Row<'_>) -> Result<(), Error> {
        let entry = row.entry;
        let name = resolve::chosen_name(&entry.names);
        let diagnostics: Vec<&str> = entry.diagnostics.iter().map(|d| code(d.code)).collect();
        self.csv.write_record([
            entry.file_ref.entry().to_string(),
            entry.file_ref.sequence().to_string(),
            entry.in_use.to_string(),
            name.map(|n| cell_text(&n.name.to_string()))
                .unwrap_or_default(),
            path_cell(&row.resolution),
            path_state(&row.resolution).to_string(),
            baseline_name(row.baseline).unwrap_or_default().to_string(),
            findings_cell(&row.findings),
            entry.si_created.map(Filetime::iso8601).unwrap_or_default(),
            name.map(|n| n.created.iso8601()).unwrap_or_default(),
            diagnostics.join(";"),
            "mft".to_string(),
            String::new(),
            String::new(),
            String::new(),
        ])?;
        Ok(())
    }

    /// One row per USN record.
    pub fn write_usn(&mut self, row: &UsnRow<'_>) -> Result<(), Error> {
        let event = match &row.record {
            usn_parse::Record::Event(event) => event,
            usn_parse::Record::Diagnostic(d) => {
                let mut cells: [&str; HEADER.len()] = [""; HEADER.len()];
                cells[5] = path_state(&row.directory);
                cells[10] = usn_code(d.code);
                cells[11] = "usn";
                self.csv.write_record(cells)?;
                return Ok(());
            }
        };
        self.csv.write_record([
            event.file.entry().to_string(),
            event.file.sequence().to_string(),
            String::new(),
            cell_text(&event.name.to_string()),
            usn_path(row, event)
                .map(|p| cell_text(&p))
                .unwrap_or_default(),
            path_state(&row.directory).to_string(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "usn".to_string(),
            event.usn.to_string(),
            reason_names(event.reason).join(";"),
            event.time.iso8601(),
        ])?;
        Ok(())
    }

    /// Flushes and returns the underlying writer.
    pub fn finish(self) -> Result<W, Error> {
        self.csv
            .into_inner()
            .map_err(|e| Error::Csv(csv::Error::from(e.into_error())))
    }
}

/// `\` plus the escaped names joined by `\` (a `\` inside a name shows as `\\`); empty if unknown.
fn path_cell(resolution: &Resolution<'_>) -> String {
    resolution
        .path_text()
        .map(|p| cell_text(&p))
        .unwrap_or_default()
}

/// `USN_REASON_*` flag names without the prefix, in bit order; unknown bits as `0x…`.
fn reason_names(flags: u32) -> Vec<String> {
    (0..32)
        .map(|bit| 1u32 << bit)
        .filter(|&flag| flags & flag != 0)
        .map(
            |flag| match REASONS.iter().find(|&&(known, _)| known == flag) {
                Some(&(_, name)) => name.to_string(),
                None => format!("0x{flag:08X}"),
            },
        )
        .collect()
}

/// Microsoft's `USN_REASON_*` flags (winioctl.h).
const REASONS: [(u32, &str); 24] = [
    (0x0000_0001, "DATA_OVERWRITE"),
    (0x0000_0002, "DATA_EXTEND"),
    (0x0000_0004, "DATA_TRUNCATION"),
    (0x0000_0010, "NAMED_DATA_OVERWRITE"),
    (0x0000_0020, "NAMED_DATA_EXTEND"),
    (0x0000_0040, "NAMED_DATA_TRUNCATION"),
    (0x0000_0100, "FILE_CREATE"),
    (0x0000_0200, "FILE_DELETE"),
    (0x0000_0400, "EA_CHANGE"),
    (0x0000_0800, "SECURITY_CHANGE"),
    (0x0000_1000, "RENAME_OLD_NAME"),
    (0x0000_2000, "RENAME_NEW_NAME"),
    (0x0000_4000, "INDEXABLE_CHANGE"),
    (0x0000_8000, "BASIC_INFO_CHANGE"),
    (0x0001_0000, "HARD_LINK_CHANGE"),
    (0x0002_0000, "COMPRESSION_CHANGE"),
    (0x0004_0000, "ENCRYPTION_CHANGE"),
    (0x0008_0000, "OBJECT_ID_CHANGE"),
    (0x0010_0000, "REPARSE_POINT_CHANGE"),
    (0x0020_0000, "STREAM_CHANGE"),
    (0x0040_0000, "TRANSACTED_CHANGE"),
    (0x0080_0000, "INTEGRITY_CHANGE"),
    (0x0100_0000, "DESIRED_STORAGE_CLASS_CHANGE"),
    (0x8000_0000, "CLOSE"),
];

/// The parent directory's path plus the record's name; `None` if the parent is unknown.
fn usn_path(row: &UsnRow<'_>, event: &usn_parse::UsnEvent) -> Option<String> {
    match &row.directory {
        Resolution::Resolved(segments) if segments.is_empty() => Some(format!("\\{}", event.name)),
        directory => Some(format!("{}\\{}", directory.path_text()?, event.name)),
    }
}

fn baseline_name(status: Option<Status>) -> Option<&'static str> {
    match status? {
        Status::Standard => Some("standard"),
        Status::Outside => Some("outside"),
    }
}

fn path_state(resolution: &Resolution<'_>) -> &'static str {
    match resolution {
        Resolution::Resolved(_) => "resolved",
        Resolution::Inferred(_) => "inferred",
        Resolution::Unknown => "unknown",
    }
}

/// `level:id` per matched rule, most severe first (the order `sigma` returns), joined by `;`.
fn findings_cell(findings: &[Finding]) -> String {
    let items: Vec<String> = findings
        .iter()
        .map(|f| format!("{}:{}", level_name(f.level), f.id))
        .collect();
    cell_text(&items.join(";")) // rule ids come from rule files: escape them like evidence
}

fn level_name(level: Option<Level>) -> &'static str {
    match level {
        Some(Level::Critical) => "critical",
        Some(Level::High) => "high",
        Some(Level::Medium) => "medium",
        Some(Level::Low) => "low",
        Some(Level::Informational) => "informational",
        None => "none",
    }
}

/// Evidence text for display (ADR 0002 #4): control characters and bidi controls (which can
/// make `exe.pdf` read as `fdp.exe`) escaped as `\u{XX}`.
fn display_text(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_control()
            || matches!(c, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
        {
            let _ = write!(out, "\\u{{{:02X}}}", u32::from(c)); // writing to a String can't fail
        } else {
            out.push(c);
        }
    }
    out
}

/// Evidence text as a CSV cell: [`display_text`], and a leading `=`, `+`, `-` or `@` prefixed
/// with `'` so spreadsheets don't run it as a formula.
fn cell_text(text: &str) -> String {
    let mut out = display_text(text);
    if out.starts_with(['=', '+', '-', '@']) {
        out.insert(0, '\'');
    }
    out
}

fn usn_code(code: usn_parse::DiagCode) -> &'static str {
    match code {
        usn_parse::DiagCode::UnsupportedVersion => "unsupported_version",
        usn_parse::DiagCode::Malformed => "malformed",
        usn_parse::DiagCode::Truncated => "truncated",
    }
}

fn code(code: DiagCode) -> &'static str {
    match code {
        DiagCode::BadSignature => "bad_signature",
        DiagCode::FixupMismatch => "fixup_mismatch",
        DiagCode::Malformed => "malformed",
        DiagCode::Truncated => "truncated",
        DiagCode::OrphanExtension => "orphan_extension",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mft_parse::{Diagnostic, Entry, FileName, Namespace};
    use ntfs_types::{FileRef, Filetime, NtfsName};

    fn units(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    fn csv_of(entries: &[Entry]) -> Result<String, Box<dyn std::error::Error>> {
        csv_with(entries, None, None)
    }

    fn csv_with(
        entries: &[Entry],
        baseline: Option<&baseline::Baseline>,
        rules: Option<&sigma::Rules>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let mut w = CsvWriter::new(Vec::new())?;
        for row in analyze::rows(entries, baseline, rules) {
            w.write(&row)?;
        }
        Ok(String::from_utf8(w.finish()?)?)
    }

    #[test]
    fn writes_header_and_one_row_per_entry() -> Result<(), Box<dyn std::error::Error>> {
        let full = Entry {
            file_ref: FileRef::from_raw(42 | (3 << 48)),
            in_use: true,
            is_dir: false,
            base: None,
            si_created: Some(Filetime::from_raw(133_444_555_666_777_888)),
            names: vec![
                FileName {
                    name: NtfsName::from_units(&units("LONGNA~1.TXT")),
                    parent: FileRef::from_raw(5),
                    namespace: Namespace::Dos,
                    created: Filetime::from_raw(0),
                },
                FileName {
                    name: NtfsName::from_units(&units(r"a\b.txt")),
                    parent: FileRef::from_raw(5),
                    namespace: Namespace::Win32,
                    created: Filetime::from_raw(133_536_836_961_234_567),
                },
            ],
            diagnostics: vec![Diagnostic {
                code: DiagCode::FixupMismatch,
                offset: 43_008,
            }],
        };
        let root = Entry {
            file_ref: FileRef::from_raw(5),
            in_use: true,
            is_dir: false,
            base: None,
            si_created: None,
            names: vec![FileName {
                name: NtfsName::from_units(&units(".")),
                parent: FileRef::from_raw(5),
                namespace: Namespace::Win32AndDos,
                created: Filetime::from_raw(0),
            }],
            diagnostics: vec![],
        };
        let empty = Entry {
            file_ref: FileRef::from_raw(43),
            in_use: false,
            is_dir: false,
            base: None,
            si_created: None,
            names: vec![],
            diagnostics: vec![
                Diagnostic {
                    code: DiagCode::BadSignature,
                    offset: 44_032,
                },
                Diagnostic {
                    code: DiagCode::Malformed,
                    offset: 44_032,
                },
            ],
        };

        assert_eq!(
            csv_of(&[root, full, empty])?,
            "entry,sequence,in_use,name,path,path_state,baseline,findings,si_created,fn_created,diagnostics,source,usn,reasons,event_time\n\
             5,0,true,.,\\,resolved,,,,1601-01-01T00:00:00.0000000Z,,mft,,,\n\
             42,3,true,a\\\\b.txt,\\a\\\\b.txt,resolved,,,2023-11-14T17:12:46.6777888Z,2024-02-29T12:34:56.1234567Z,fixup_mismatch,mft,,,\n\
             43,0,false,,,unknown,,,,,bad_signature;malformed,mft,,,\n"
        );
        Ok(())
    }

    fn named(name: &str) -> Entry {
        Entry {
            file_ref: FileRef::from_raw(0),
            in_use: true,
            is_dir: false,
            base: None,
            si_created: None,
            names: vec![FileName {
                name: NtfsName::from_units(&units(name)),
                parent: FileRef::from_raw(5),
                namespace: Namespace::Win32,
                created: Filetime::from_raw(0),
            }],
            diagnostics: vec![],
        }
    }

    #[test]
    fn neutralizes_formulas_and_escapes_control_characters_in_names()
    -> Result<(), Box<dyn std::error::Error>> {
        let cases = [
            ("=cmd|' /C calc'!A0", "'=cmd|' /C calc'!A0"),
            ("+1", "'+1"),
            ("-1", "'-1"),
            ("@SUM(1)", "'@SUM(1)"),
            ("x=1", "x=1"),
            ("\tx", r"\u{09}x"),
            ("\r=x", r"\u{0D}=x"),
            ("a\nb", r"a\u{0A}b"),
            ("a\u{1b}[31mb", r"a\u{1B}[31mb"),
            ("a\u{85}b", r"a\u{85}b"),
            ("x\u{202E}fdp.exe", r"x\u{202E}fdp.exe"),
            (
                "\u{200E}\u{200F}\u{202A}\u{202B}\u{202C}\u{202D}",
                r"\u{200E}\u{200F}\u{202A}\u{202B}\u{202C}\u{202D}",
            ),
            (
                "\u{2066}\u{2067}\u{2068}\u{2069}",
                r"\u{2066}\u{2067}\u{2068}\u{2069}",
            ),
            ("\u{2065}\u{206A}\u{2029}", "\u{2065}\u{206A}\u{2029}"),
            ("a,b", r#""a,b""#),
            ("a\"b", r#""a""b""#),
        ];
        for (name, cell) in cases {
            let csv = csv_of(&[named(name)])?;
            let row = csv.lines().nth(1).unwrap_or_default();
            assert_eq!(
                row,
                format!("0,0,true,{cell},,unknown,,,,1601-01-01T00:00:00.0000000Z,,mft,,,"),
                "name {name:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn escapes_control_characters_in_path_segments() -> Result<(), Box<dyn std::error::Error>> {
        let mut root = named(".");
        root.file_ref = FileRef::from_raw(5);
        let mut file = named("x\u{1b}[2J.txt");
        file.file_ref = FileRef::from_raw(40);

        let csv = csv_of(&[root, file])?;

        let path = csv.lines().nth(2).and_then(|row| row.split(',').nth(4));
        assert_eq!(path, Some(r"\x\u{1B}[2J.txt"));
        Ok(())
    }

    #[test]
    fn writes_the_baseline_status() -> Result<(), Box<dyn std::error::Error>> {
        let vwr = "\"DirectoryName\",\"Name\",\"FullName\"\n\"C:\\\",\"x.txt\",\"C:\\x.txt\"\n";
        let mut file = Vec::new();
        baseline::build(vwr.as_bytes(), &mut file)?;
        let baseline = baseline::Baseline::load(file)?;
        let mut root = named(".");
        root.file_ref = FileRef::from_raw(5);
        root.is_dir = true;
        let mut standard = named("X.TXT");
        standard.file_ref = FileRef::from_raw(40);
        let mut outside = named("y.txt");
        outside.file_ref = FileRef::from_raw(41);

        let csv = csv_with(&[root, standard, outside], Some(&baseline), None)?;

        let column: Vec<&str> = csv
            .lines()
            .map(|row| row.split(',').nth(6).unwrap_or_default())
            .collect();
        assert_eq!(column, ["baseline", "", "standard", "outside"]);
        Ok(())
    }

    #[test]
    fn writes_the_orphan_extension_code() -> Result<(), Box<dyn std::error::Error>> {
        let mut orphan = named("x");
        orphan.diagnostics = vec![Diagnostic {
            code: DiagCode::OrphanExtension,
            offset: 0,
        }];

        let csv = csv_of(&[orphan])?;

        assert!(
            csv.lines()
                .nth(1)
                .is_some_and(|row| row.ends_with(",orphan_extension,mft,,,")),
            "{csv}"
        );
        Ok(())
    }

    #[test]
    fn writes_findings_as_level_and_id() -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!("report-rules-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let rule = |id: &str, level: &str| {
            format!(
                "title: {id}\nid: {id}\nlevel: {level}\nlogsource:\n  product: windows\n  category: file_event\ndetection:\n  sel:\n    TargetFilename|endswith: '.exe'\n  condition: sel\n"
            )
        };
        std::fs::write(dir.join("a.yml"), rule("rule-low", "low"))?;
        std::fs::write(dir.join("b.yml"), rule("rule-high", "high"))?;
        let rules = sigma::Rules::load(&dir)?;
        let mut root = named(".");
        root.file_ref = FileRef::from_raw(5);
        root.is_dir = true;
        let mut exe = named("x.exe");
        exe.file_ref = FileRef::from_raw(40);

        let csv = csv_with(&[root, exe], None, Some(&rules))?;

        let column: Vec<&str> = csv
            .lines()
            .map(|row| row.split(',').nth(7).unwrap_or_default())
            .collect();
        assert_eq!(column, ["findings", "", "high:rule-high;low:rule-low"]);
        Ok(())
    }

    #[test]
    fn escapes_control_characters_in_rule_ids() -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!("report-rule-id-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            dir.join("a.yml"),
            "title: T\nid: \"evil\\e[2J\"\nlevel: high\nlogsource:\n  product: windows\n  category: file_event\ndetection:\n  sel:\n    TargetFilename|endswith: '.exe'\n  condition: sel\n",
        )?;
        let rules = sigma::Rules::load(&dir)?;
        let mut root = named(".");
        root.file_ref = FileRef::from_raw(5);
        root.is_dir = true;
        let mut exe = named("x.exe");
        exe.file_ref = FileRef::from_raw(40);

        let csv = csv_with(&[root, exe], None, Some(&rules))?;

        let cell = csv.lines().nth(2).and_then(|row| row.split(',').nth(7));
        assert_eq!(cell, Some(r"high:evil\u{1B}[2J"));
        Ok(())
    }

    fn dir(number: u64, parent: u64, name: &str) -> Entry {
        let mut e = named(name);
        e.file_ref = FileRef::from_raw(number | (1 << 48));
        e.is_dir = true;
        e.names[0].parent = FileRef::from_raw(parent | (1 << 48));
        e
    }

    /// A small tree (`\`, `\Users`) to resolve USN parents against.
    fn usn_tree() -> Vec<Entry> {
        vec![dir(5, 5, "."), dir(6, 5, "Users")]
    }

    fn usn_event(file: u64, parent: u64, usn: u64, reason: u32, name: &str) -> usn_parse::Record {
        usn_parse::Record::Event(usn_parse::UsnEvent {
            offset: usn,
            usn,
            file: FileRef::from_raw(file),
            parent: FileRef::from_raw(parent),
            time: Filetime::from_raw(133_444_555_666_777_888),
            reason,
            attributes: 0x20,
            name: NtfsName::from_units(&units(name)),
        })
    }

    fn usn_csv(
        entries: &[Entry],
        records: Vec<usn_parse::Record>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let mut w = CsvWriter::new(Vec::new())?;
        for row in analyze::usn_rows(entries, &records) {
            w.write_usn(&row)?;
        }
        Ok(String::from_utf8(w.finish()?)?
            .lines()
            .skip(1)
            .map(|l| format!("{l}\n"))
            .collect())
    }

    #[test]
    fn csv_writes_a_usn_event_row() -> Result<(), Box<dyn std::error::Error>> {
        let records = vec![
            usn_event(60 | (1 << 48), 6 | (1 << 48), 4096, 0, "a.txt"),
            usn_event(61 | (2 << 48), 77 | (1 << 48), 4160, 0, "b.txt"),
            usn_event(62 | (1 << 48), 5 | (1 << 48), 4224, 0, "c.txt"),
        ];

        assert_eq!(
            usn_csv(&usn_tree(), records)?,
            "60,1,,a.txt,\\Users\\a.txt,resolved,,,,,,usn,4096,,2023-11-14T17:12:46.6777888Z\n\
             61,2,,b.txt,,unknown,,,,,,usn,4160,,2023-11-14T17:12:46.6777888Z\n\
             62,1,,c.txt,\\c.txt,resolved,,,,,,usn,4224,,2023-11-14T17:12:46.6777888Z\n"
        );
        Ok(())
    }

    #[test]
    fn usn_path_keeps_a_backslash_at_the_end_of_a_directory_name()
    -> Result<(), Box<dyn std::error::Error>> {
        let entries = vec![dir(5, 5, "."), dir(6, 5, "odd\\")];
        let records = vec![usn_event(60 | (1 << 48), 6 | (1 << 48), 0, 0, "b.txt")];

        let csv = usn_csv(&entries, records)?;

        assert_eq!(csv.split(',').nth(4), Some(r"\odd\\\b.txt"));
        Ok(())
    }

    #[test]
    fn reason_flags_become_names_in_bit_order_and_unknown_bits_hex()
    -> Result<(), Box<dyn std::error::Error>> {
        let all_known = [
            (0x0000_0001, "DATA_OVERWRITE"),
            (0x0000_0002, "DATA_EXTEND"),
            (0x0000_0004, "DATA_TRUNCATION"),
            (0x0000_0010, "NAMED_DATA_OVERWRITE"),
            (0x0000_0020, "NAMED_DATA_EXTEND"),
            (0x0000_0040, "NAMED_DATA_TRUNCATION"),
            (0x0000_0100, "FILE_CREATE"),
            (0x0000_0200, "FILE_DELETE"),
            (0x0000_0400, "EA_CHANGE"),
            (0x0000_0800, "SECURITY_CHANGE"),
            (0x0000_1000, "RENAME_OLD_NAME"),
            (0x0000_2000, "RENAME_NEW_NAME"),
            (0x0000_4000, "INDEXABLE_CHANGE"),
            (0x0000_8000, "BASIC_INFO_CHANGE"),
            (0x0001_0000, "HARD_LINK_CHANGE"),
            (0x0002_0000, "COMPRESSION_CHANGE"),
            (0x0004_0000, "ENCRYPTION_CHANGE"),
            (0x0008_0000, "OBJECT_ID_CHANGE"),
            (0x0010_0000, "REPARSE_POINT_CHANGE"),
            (0x0020_0000, "STREAM_CHANGE"),
            (0x0040_0000, "TRANSACTED_CHANGE"),
            (0x0080_0000, "INTEGRITY_CHANGE"),
            (0x0100_0000, "DESIRED_STORAGE_CLASS_CHANGE"),
            (0x8000_0000, "CLOSE"),
        ];
        for (bit, name) in all_known {
            assert_eq!(reason_names(bit), [name]);
        }
        assert_eq!(reason_names(0), Vec::<String>::new());
        assert_eq!(
            reason_names(0xC000_010B),
            [
                "DATA_OVERWRITE",
                "DATA_EXTEND",
                "0x00000008",
                "FILE_CREATE",
                "0x40000000",
                "CLOSE"
            ]
        );

        let csv = usn_csv(
            &usn_tree(),
            vec![usn_event(60, 6 | (1 << 48), 0, 0x8000_0100, "a")],
        )?;
        assert_eq!(csv.split(',').nth(13), Some("FILE_CREATE;CLOSE"));
        Ok(())
    }

    #[test]
    fn usn_names_are_escaped_like_mft_names() -> Result<(), Box<dyn std::error::Error>> {
        let records = vec![
            usn_event(60, 6 | (1 << 48), 0, 0, "=cmd|' /C calc'!A0"),
            usn_event(61, 6 | (1 << 48), 8, 0, "x\u{1b}[2J\u{202E}fdp.exe"),
        ];

        let csv = usn_csv(&usn_tree(), records)?;
        let cells: Vec<Vec<&str>> = csv
            .lines()
            .map(|l| l.split(',').skip(3).take(2).collect())
            .collect();

        assert_eq!(
            cells,
            [
                vec!["'=cmd|' /C calc'!A0", r"\Users\=cmd|' /C calc'!A0"],
                vec![
                    r"x\u{1B}[2J\u{202E}fdp.exe",
                    r"\Users\x\u{1B}[2J\u{202E}fdp.exe"
                ],
            ]
        );
        Ok(())
    }

    #[test]
    fn usn_diagnostics_become_rows() -> Result<(), Box<dyn std::error::Error>> {
        let diagnostic =
            |code, offset| usn_parse::Record::Diagnostic(usn_parse::Diagnostic { code, offset });
        let records = || {
            vec![
                diagnostic(usn_parse::DiagCode::Malformed, 8),
                diagnostic(usn_parse::DiagCode::Truncated, 16),
                diagnostic(usn_parse::DiagCode::UnsupportedVersion, 24),
            ]
        };

        let csv = usn_csv(&usn_tree(), records())?;
        let mut jsonl = JsonlWriter::new(Vec::new());
        let (tree, records) = (usn_tree(), records());
        for row in analyze::usn_rows(&tree, &records) {
            jsonl.write_usn(&row)?;
        }

        assert_eq!(
            csv,
            ",,,,,unknown,,,,,malformed,usn,,,\n\
             ,,,,,unknown,,,,,truncated,usn,,,\n\
             ,,,,,unknown,,,,,unsupported_version,usn,,,\n"
        );
        assert_eq!(
            String::from_utf8(jsonl.finish()?)?.lines().next(),
            Some(
                r#"{"entry":null,"sequence":null,"in_use":null,"name":null,"path":null,"path_state":"unknown","baseline":null,"findings":[],"si_created":null,"fn_created":null,"diagnostics":["malformed"],"source":"usn","usn":null,"reasons":[],"event_time":null}"#
            )
        );
        Ok(())
    }

    #[test]
    fn inferred_paths_are_marked_in_csv_and_jsonl() -> Result<(), Box<dyn std::error::Error>> {
        let entries = vec![dir(5, 5, "."), dir(6, 5, "New")];
        let records = || {
            vec![
                usn_event(60 | (1 << 48), 6 | (1 << 48), 0, 0, "a.txt"),
                usn_event(6 | (1 << 48), 5 | (1 << 48), 8, 0x1000, "Old"),
                usn_event(6 | (1 << 48), 5 | (1 << 48), 16, 0x2000, "New"),
            ]
        };

        let csv = usn_csv(&entries, records())?;
        let mut jsonl = JsonlWriter::new(Vec::new());
        let records = records();
        for row in analyze::usn_rows(&entries, &records) {
            jsonl.write_usn(&row)?;
        }
        let jsonl = String::from_utf8(jsonl.finish()?)?;

        assert_eq!(
            csv.lines()
                .next()
                .map(|l| l.split(',').skip(4).take(2).collect::<Vec<_>>()),
            Some(vec![r"\Old\a.txt", "inferred"])
        );
        assert!(
            jsonl
                .lines()
                .next()
                .is_some_and(|l| l.contains(r#""path":"\\Old\\a.txt","path_state":"inferred""#)),
            "{jsonl}"
        );
        Ok(())
    }
}
