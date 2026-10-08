//! Self-contained HTML report: a static template plus the report data as Base64 JSON.

use std::io::{BufWriter, Write};

use analyze::Row;
use baseline::Status;
use mft_parse::DiagCode;
use ntfs_types::Filetime;
use resolve::Resolution;
use serde::{Serialize, Serializer};
use sigma::Level;

use crate::base64::Encoder;
use crate::{Error, display_text, level_name};

const TEMPLATE: &str = include_str!("template.html");
const PLACEHOLDER: &str = "__REPORT_DATA__";

/// Build facts shown in the footer (AGPL notice, ADR 0001).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Provenance {
    pub version: &'static str,
    /// Source commit, or `unknown`.
    pub commit: &'static str,
    pub source_url: &'static str,
}

/// Summary facts that the rows don't carry.
#[derive(Debug)]
pub struct Input {
    /// Input file name, without its directory.
    pub name: String,
    pub record_size: u64,
    /// Baseline file name, if one was used.
    pub baseline: Option<String>,
    pub rules: usize,
}

/// Collects rows, then writes one HTML file.
#[derive(Debug)]
pub struct HtmlReport {
    provenance: Provenance,
    summary: Summary,
    findings: Vec<FindingItem>,
    outside: Vec<OutsideItem>,
}

impl HtmlReport {
    pub fn new(provenance: Provenance, input: Input) -> Self {
        let summary = Summary {
            input: display_text(&input.name),
            host: "not available ($MFT only)",
            record_size: input.record_size,
            entries: 0,
            files: 0,
            resolved: 0,
            unknown: 0,
            diagnostics: Diagnostics::default(),
            baseline: input.baseline.as_deref().map(display_text),
            outside: 0,
            rules: input.rules,
            findings: Levels::default(),
        };
        Self {
            provenance,
            summary,
            findings: Vec::new(),
            outside: Vec::new(),
        }
    }

    pub fn add(&mut self, row: &Row<'_>) {
        let s = &mut self.summary;
        s.entries += 1;
        if row.entry.in_use && !row.entry.is_dir {
            s.files += 1;
        }
        match row.resolution {
            Resolution::Resolved(_) => s.resolved += 1,
            Resolution::Unknown => s.unknown += 1,
        }
        for diagnostic in &row.entry.diagnostics {
            let d = &mut s.diagnostics;
            *match diagnostic.code {
                DiagCode::BadSignature => &mut d.bad_signature,
                DiagCode::FixupMismatch => &mut d.fixup_mismatch,
                DiagCode::Malformed => &mut d.malformed,
                DiagCode::Truncated => &mut d.truncated,
                DiagCode::OrphanExtension => &mut d.orphan_extension,
            } += 1;
        }
        if row.baseline == Some(Status::Outside) {
            s.outside += 1;
            self.outside.push(OutsideItem {
                created: row.entry.si_created,
                path: path_text(&row.resolution),
                entry: row.entry.file_ref.entry(),
            });
        }
        for finding in &row.findings {
            let l = &mut s.findings;
            *match finding.level {
                Some(Level::Critical) => &mut l.critical,
                Some(Level::High) => &mut l.high,
                Some(Level::Medium) => &mut l.medium,
                Some(Level::Low) => &mut l.low,
                Some(Level::Informational) => &mut l.informational,
                None => &mut l.none,
            } += 1;
            self.findings.push(FindingItem {
                level: finding.level,
                title: display_text(&finding.title),
                id: display_text(&finding.id),
                author: finding.author.as_deref().map(display_text),
                created: row.entry.si_created,
                path: path_text(&row.resolution),
                entry: row.entry.file_ref.entry(),
                explain: display_text(&finding.explain),
            });
        }
    }

    /// Writes the report and returns the writer.
    pub fn finish<W: Write>(mut self, mut out: W) -> Result<W, Error> {
        // Level, then newest first (no time last), then a total order for byte-identical output.
        self.findings.sort_by(|a, b| {
            (b.level, b.created)
                .cmp(&(a.level, a.created))
                .then_with(|| (&a.path, a.entry, &a.id).cmp(&(&b.path, b.entry, &b.id)))
        });
        self.outside.sort_by(|a, b| {
            b.created
                .cmp(&a.created)
                .then_with(|| (&a.path, a.entry).cmp(&(&b.path, b.entry)))
        });
        let data = Data {
            summary: self.summary,
            findings: self.findings,
            outside: self.outside,
            footer: Footer {
                provenance: self.provenance,
                license: "AGPL-3.0-only",
            },
        };
        let (head, tail) = TEMPLATE.split_once(PLACEHOLDER).unwrap_or((TEMPLATE, ""));
        out.write_all(head.as_bytes())?;
        // Streamed: the JSON and its Base64 are each about as large as the report (Phase 1 gate).
        // The buffer hands the encoder large chunks instead of serde's many small writes.
        let mut encoder = Encoder::new(out);
        let mut json = BufWriter::with_capacity(1 << 16, &mut encoder);
        serde_json::to_writer(&mut json, &data).map_err(std::io::Error::from)?;
        json.flush()?;
        drop(json);
        let mut out = encoder.finish()?;
        out.write_all(tail.as_bytes())?;
        Ok(out)
    }
}

/// One file outside the baseline.
#[derive(Debug, Serialize)]
struct OutsideItem {
    /// `$SI` created time.
    #[serde(serialize_with = "iso8601")]
    created: Option<Filetime>,
    path: String,
    entry: u64,
}

fn iso8601<S: Serializer>(time: &Option<Filetime>, s: S) -> Result<S::Ok, S::Error> {
    time.map(Filetime::iso8601).serialize(s)
}

fn level<S: Serializer>(level: &Option<Level>, s: S) -> Result<S::Ok, S::Error> {
    level_name(*level).serialize(s)
}

/// The resolved path as display text; empty if unknown.
fn path_text(resolution: &Resolution<'_>) -> String {
    resolution
        .path_text()
        .map(|p| display_text(&p))
        .unwrap_or_default()
}

/// The embedded JSON; field order is the output order.
#[derive(Serialize)]
struct Data {
    summary: Summary,
    findings: Vec<FindingItem>,
    outside: Vec<OutsideItem>,
    footer: Footer,
}

#[derive(Serialize)]
struct Footer {
    #[serde(flatten)]
    provenance: Provenance,
    license: &'static str,
}

/// One matched rule on one file.
#[derive(Debug, Serialize)]
struct FindingItem {
    #[serde(serialize_with = "level")]
    level: Option<Level>,
    title: String,
    id: String,
    author: Option<String>,
    /// `$SI` created time.
    #[serde(serialize_with = "iso8601")]
    created: Option<Filetime>,
    path: String,
    entry: u64,
    explain: String,
}

#[derive(Debug, Serialize)]
struct Summary {
    input: String,
    host: &'static str,
    record_size: u64,
    entries: u64,
    /// In-use, non-directory entries.
    files: u64,
    resolved: u64,
    unknown: u64,
    diagnostics: Diagnostics,
    baseline: Option<String>,
    outside: u64,
    rules: usize,
    findings: Levels,
}

/// Count per diagnostic code.
#[derive(Debug, Default, Serialize)]
struct Diagnostics {
    bad_signature: u64,
    fixup_mismatch: u64,
    malformed: u64,
    truncated: u64,
    orphan_extension: u64,
}

/// Count of findings per level.
#[derive(Debug, Default, Serialize)]
struct Levels {
    critical: u64,
    high: u64,
    medium: u64,
    low: u64,
    informational: u64,
    none: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use mft_parse::{Diagnostic, Entry, FileName, Namespace};
    use ntfs_types::{FileRef, NtfsName};

    type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

    const PROVENANCE: Provenance = Provenance {
        version: "1.2.3",
        commit: "0123456789abcdef0123456789abcdef01234567",
        source_url: "https://example.com/src",
    };

    fn input() -> Input {
        Input {
            name: "MFT".to_string(),
            record_size: 1024,
            baseline: None,
            rules: 0,
        }
    }

    fn html_of(entries: &[Entry], input: Input) -> Result<String> {
        html_with(entries, None, None, input)
    }

    fn html_with(
        entries: &[Entry],
        baseline: Option<&baseline::Baseline>,
        rules: Option<&sigma::Rules>,
        input: Input,
    ) -> Result<String> {
        let mut report = HtmlReport::new(PROVENANCE, input);
        for row in analyze::rows(entries, baseline, rules) {
            report.add(&row);
        }
        Ok(String::from_utf8(report.finish(Vec::new())?)?)
    }

    /// The Base64 payload between the template's halves.
    fn payload(html: &str) -> Result<&str> {
        let (head, tail) = TEMPLATE.split_once(PLACEHOLDER).ok_or("no placeholder")?;
        let rest = html
            .strip_prefix(head)
            .ok_or("head differs from the template")?;
        Ok(rest
            .strip_suffix(tail)
            .ok_or("tail differs from the template")?)
    }

    fn decode(text: &str) -> Result<serde_json::Value> {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut bits = 0u32;
        let mut count = 0;
        let mut bytes = Vec::new();
        for c in text.bytes().filter(|&c| c != b'=') {
            let value = ALPHABET.iter().position(|&a| a == c).ok_or("not Base64")?;
            bits = bits << 6 | u32::try_from(value)?;
            count += 6;
            if count >= 8 {
                count -= 8;
                bytes.push(u8::try_from(bits >> count & 0xff)?);
            }
        }
        Ok(serde_json::from_slice(&bytes)?)
    }

    fn units(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    fn file(number: u64, parent: u64, name: &str) -> Entry {
        Entry {
            file_ref: FileRef::from_raw(number),
            in_use: true,
            is_dir: false,
            base: None,
            si_created: None,
            names: vec![FileName {
                name: NtfsName::from_units(&units(name)),
                parent: FileRef::from_raw(parent),
                namespace: Namespace::Win32,
                created: Filetime::from_raw(0),
            }],
            diagnostics: vec![],
        }
    }

    fn root() -> Entry {
        let mut root = file(5, 5, ".");
        root.is_dir = true;
        root
    }

    #[test]
    fn embeds_the_data_as_one_base64_payload_in_the_template() -> Result<()> {
        assert_eq!(TEMPLATE.matches(PLACEHOLDER).count(), 1);

        let html = html_of(&[root(), file(40, 5, "a.txt")], input())?;

        let payload = payload(&html)?;
        assert!(
            !payload.is_empty()
                && payload
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'/' | b'=')),
            "{payload}"
        );
        let data = decode(payload)?;
        assert_eq!(data["summary"]["input"], "MFT");
        Ok(())
    }

    /// Rules in a fresh directory: (id, level or "", author or "", file suffix).
    fn rules(test: &str, specs: &[(&str, &str, &str, &str)]) -> Result<sigma::Rules> {
        let dir = std::env::temp_dir().join(format!("report-html-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        for (id, level, author, suffix) in specs {
            let level = if level.is_empty() {
                String::new()
            } else {
                format!("level: {level}\n")
            };
            let author = if author.is_empty() {
                String::new()
            } else {
                format!("author: {author}\n")
            };
            std::fs::write(
                dir.join(format!("{id}.yml")),
                format!(
                    "title: Title {id}\nid: {id}\n{author}{level}logsource:\n  product: windows\n  category: file_event\ndetection:\n  sel:\n    TargetFilename|endswith: '{suffix}'\n  condition: sel\n"
                ),
            )?;
        }
        Ok(sigma::Rules::load(&dir)?)
    }

    fn created(mut entry: Entry, raw: u64) -> Entry {
        entry.si_created = Some(Filetime::from_raw(raw));
        entry
    }

    #[test]
    fn findings_list_each_rule_match_by_level_then_newest_first() -> Result<()> {
        let rules = rules(
            "findings",
            &[
                ("rule-high", "high", "alice", ".exe"),
                ("rule-low", "low", "", ".exe"),
                ("rule-medium", "medium", "bob", ".dll"),
                ("rule-none", "", "", ".dll"),
            ],
        )?;
        let entries = [
            root(),
            created(file(40, 5, "a.exe"), 100),
            created(file(41, 5, "d.exe"), 200),
            created(file(42, 5, "b.exe"), 200),
            file(43, 5, "c.exe"),
            created(file(44, 5, "z.dll"), 100),
        ];

        let html = html_with(&entries, None, Some(&rules), input())?;

        let data = decode(payload(&html)?)?;
        let order: Vec<String> = data["findings"]
            .as_array()
            .ok_or("no findings")?
            .iter()
            .map(|f| {
                format!(
                    "{} {}",
                    f["level"].as_str().unwrap_or("?"),
                    f["path"].as_str().unwrap_or("?")
                )
            })
            .collect();
        assert_eq!(
            order,
            [
                r"high \b.exe",
                r"high \d.exe",
                r"high \a.exe",
                r"high \c.exe",
                r"medium \z.dll",
                r"low \b.exe",
                r"low \d.exe",
                r"low \a.exe",
                r"low \c.exe",
                r"none \z.dll",
            ]
        );
        let first = &data["findings"][0];
        assert_eq!(first["title"], "Title rule-high");
        assert_eq!(first["id"], "rule-high");
        assert_eq!(first["author"], "alice");
        assert_eq!(first["created"], "1601-01-01T00:00:00.0000200Z");
        assert_eq!(first["entry"], 42);
        assert!(
            first["explain"]
                .as_str()
                .is_some_and(|e| e.contains("b.exe")),
            "{first}"
        );
        assert_eq!(data["findings"][5]["author"], serde_json::Value::Null);
        assert_eq!(data["findings"][3]["created"], serde_json::Value::Null);
        assert_eq!(
            data["summary"]["findings"],
            serde_json::json!({"critical": 0, "high": 4, "medium": 1, "low": 4, "informational": 0, "none": 1})
        );
        Ok(())
    }

    #[test]
    fn outside_lists_only_outside_baseline_files_newest_first() -> Result<()> {
        let vwr = "\"DirectoryName\",\"Name\",\"FullName\"\n\"C:\\\",\"x.txt\",\"C:\\x.txt\"\n";
        let mut file_bytes = Vec::new();
        baseline::build(vwr.as_bytes(), &mut file_bytes)?;
        let baseline = baseline::Baseline::load(file_bytes)?;
        let entries = [
            root(),
            created(file(40, 5, "X.TXT"), 300),
            created(file(41, 5, "y.txt"), 100),
            created(file(42, 5, "w.txt"), 200),
            file(43, 5, "v.txt"),
            created(file(44, 5, "u.txt"), 200),
        ];

        let html = html_with(&entries, Some(&baseline), None, input())?;

        let data = decode(payload(&html)?)?;
        assert_eq!(
            data["outside"],
            serde_json::json!([
                {"created": "1601-01-01T00:00:00.0000200Z", "path": r"\u.txt", "entry": 44},
                {"created": "1601-01-01T00:00:00.0000200Z", "path": r"\w.txt", "entry": 42},
                {"created": "1601-01-01T00:00:00.0000100Z", "path": r"\y.txt", "entry": 41},
                {"created": null, "path": r"\v.txt", "entry": 43},
            ])
        );
        assert_eq!(data["summary"]["outside"], 4);
        Ok(())
    }

    #[test]
    fn attacker_names_never_reach_the_markup() -> Result<()> {
        let vwr = "\"DirectoryName\",\"Name\",\"FullName\"\n\"C:\\\",\"x.txt\",\"C:\\x.txt\"\n";
        let mut file_bytes = Vec::new();
        baseline::build(vwr.as_bytes(), &mut file_bytes)?;
        let baseline = baseline::Baseline::load(file_bytes)?;
        let mut surrogate = file(44, 5, "");
        surrogate.names[0].name = NtfsName::from_units(&[0x61, 0xD800, 0x62]);
        let entries = [
            root(),
            file(40, 5, "</script><script>alert(1)</script>"),
            file(41, 5, "=cmd|' /C calc'!A0"),
            file(42, 5, "bell\u{7}.txt"),
            file(43, 5, "x\u{202E}fdp.exe"),
            surrogate,
        ];

        let html = html_with(&entries, Some(&baseline), None, input())?;

        for raw in ["<script>alert(1)", "=cmd|", "\u{7}", "\u{202E}"] {
            assert!(!html.contains(raw), "{raw:?} is in the HTML");
        }
        assert_eq!(
            html.matches("</script>").count(),
            TEMPLATE.matches("</script>").count()
        );
        let data = decode(payload(&html)?)?;
        let paths: Vec<&str> = data["outside"]
            .as_array()
            .ok_or("no outside list")?
            .iter()
            .filter_map(|o| o["path"].as_str())
            .collect();
        assert_eq!(
            paths,
            [
                r"\</script><script>alert(1)</script>",
                r"\=cmd|' /C calc'!A0",
                r"\a\u{D800}b",
                r"\bell\u{07}.txt",
                r"\x\u{202E}fdp.exe",
            ]
        );
        Ok(())
    }

    #[test]
    fn template_only_builds_markup_through_the_dom_and_loads_nothing() {
        for banned in [
            "innerHTML",
            "outerHTML",
            "insertAdjacentHTML",
            "document.write",
            "eval(",
            "Function(",
            "src=",
            "href=",
            "http://",
            "https://",
        ] {
            assert!(!TEMPLATE.contains(banned), "template contains {banned:?}");
        }
        assert!(TEMPLATE.contains(
            r#"<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'">"#
        ));
    }

    #[test]
    fn footer_carries_the_license_source_and_commit() -> Result<()> {
        let html = html_of(&[root()], input())?;

        assert_eq!(
            decode(payload(&html)?)?["footer"],
            serde_json::json!({
                "version": "1.2.3",
                "commit": "0123456789abcdef0123456789abcdef01234567",
                "source_url": "https://example.com/src",
                "license": "AGPL-3.0-only",
            })
        );
        Ok(())
    }

    #[test]
    fn same_rows_give_byte_identical_html() -> Result<()> {
        let rules = rules(
            "determinism",
            &[
                ("rule-a", "high", "", ".exe"),
                ("rule-b", "high", "", ".exe"),
            ],
        )?;
        let entries = [
            root(),
            created(file(40, 5, "a.exe"), 100),
            created(file(41, 5, "b.exe"), 100),
        ];

        let first = html_with(&entries, None, Some(&rules), input())?;
        let second = html_with(&entries, None, Some(&rules), input())?;

        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn input_and_baseline_names_are_display_text() -> Result<()> {
        let input = Input {
            name: "M\u{1b}[2JFT".to_string(),
            record_size: 1024,
            baseline: Some("b\u{202E}tsf.fst".to_string()),
            rules: 0,
        };

        let summary = &decode(payload(&html_of(&[root()], input)?)?)?["summary"];

        assert_eq!(summary["input"], r"M\u{1B}[2JFT");
        assert_eq!(summary["baseline"], r"b\u{202E}tsf.fst");
        Ok(())
    }

    #[test]
    fn summary_counts_entries_files_paths_and_diagnostics() -> Result<()> {
        let mut deleted = file(41, 5, "old.txt");
        deleted.in_use = false;
        let mut dir = file(42, 5, "dir");
        dir.is_dir = true;
        let mut broken = file(43, 99, "lost.txt");
        broken.diagnostics = vec![
            Diagnostic {
                code: DiagCode::Malformed,
                offset: 0,
            },
            Diagnostic {
                code: DiagCode::OrphanExtension,
                offset: 0,
            },
        ];
        let input = Input {
            name: "MFT".to_string(),
            record_size: 4096,
            baseline: Some("win11.fst".to_string()),
            rules: 3,
        };

        let html = html_of(&[root(), file(40, 5, "a.txt"), deleted, dir, broken], input)?;

        assert_eq!(
            decode(payload(&html)?)?["summary"],
            serde_json::json!({
                "input": "MFT",
                "host": "not available ($MFT only)",
                "record_size": 4096,
                "entries": 5,
                "files": 2,
                "resolved": 4,
                "unknown": 1,
                "diagnostics": {
                    "bad_signature": 0,
                    "fixup_mismatch": 0,
                    "malformed": 1,
                    "truncated": 0,
                    "orphan_extension": 1,
                },
                "baseline": "win11.fst",
                "outside": 0,
                "rules": 3,
                "findings": {
                    "critical": 0,
                    "high": 0,
                    "medium": 0,
                    "low": 0,
                    "informational": 0,
                    "none": 0,
                },
            })
        );
        Ok(())
    }
}
