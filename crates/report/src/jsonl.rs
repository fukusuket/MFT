//! JSON Lines: one object per `$MFT` record, the CSV columns as typed fields.

use std::io::Write;

use analyze::Row;
use ntfs_types::Filetime;
use serde::Serialize;

use crate::{Error, baseline_name, code, display_text, level_name, path_state};

/// JSON Lines with one object per `$MFT` record.
#[derive(Debug)]
pub struct JsonlWriter<W: Write> {
    out: W,
}

impl<W: Write> JsonlWriter<W> {
    pub fn new(out: W) -> Self {
        Self { out }
    }

    pub fn write(&mut self, row: &Row<'_>) -> Result<(), Error> {
        let entry = row.entry;
        let name = resolve::chosen_name(&entry.names);
        let record = Record {
            entry: entry.file_ref.entry(),
            sequence: entry.file_ref.sequence(),
            in_use: entry.in_use,
            name: name.map(|n| display_text(&n.name.to_string())),
            path: row.resolution.path_text().map(|p| display_text(&p)),
            path_state: path_state(&row.resolution),
            baseline: baseline_name(row.baseline),
            findings: row
                .findings
                .iter()
                .map(|f| RuleMatch {
                    level: level_name(f.level),
                    id: display_text(&f.id), // rule ids come from rule files: escape them like evidence
                })
                .collect(),
            si_created: entry.si_created.map(Filetime::iso8601),
            fn_created: name.map(|n| n.created.iso8601()),
            diagnostics: entry.diagnostics.iter().map(|d| code(d.code)).collect(),
            source: "mft",
            usn: None,
            reasons: Vec::new(),
            event_time: None,
        };
        serde_json::to_writer(&mut self.out, &record)?;
        self.out.write_all(b"\n").map_err(serde_json::Error::io)?;
        Ok(())
    }

    /// Returns the underlying writer.
    pub fn finish(self) -> Result<W, Error> {
        Ok(self.out)
    }
}

/// One line; field order is the output order.
#[derive(Serialize)]
struct Record {
    entry: u64,
    sequence: u16,
    in_use: bool,
    name: Option<String>,
    path: Option<String>,
    path_state: &'static str,
    baseline: Option<&'static str>,
    findings: Vec<RuleMatch>,
    si_created: Option<String>,
    fn_created: Option<String>,
    diagnostics: Vec<&'static str>,
    source: &'static str,
    usn: Option<u64>,
    reasons: Vec<String>,
    event_time: Option<String>,
}

/// One matched rule, most severe first (the order `sigma` returns).
#[derive(Serialize)]
struct RuleMatch {
    level: &'static str,
    id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use mft_parse::{DiagCode, Diagnostic, Entry, FileName, Namespace};
    use ntfs_types::{FileRef, Filetime, NtfsName};

    type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

    fn units(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    fn jsonl_with(
        entries: &[Entry],
        baseline: Option<&baseline::Baseline>,
        rules: Option<&sigma::Rules>,
    ) -> Result<String> {
        let mut w = JsonlWriter::new(Vec::new());
        for row in analyze::rows(entries, baseline, rules) {
            w.write(&row)?;
        }
        Ok(String::from_utf8(w.finish()?)?)
    }

    fn named(number: u64, name: &str) -> Entry {
        Entry {
            file_ref: FileRef::from_raw(number),
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
    fn jsonl_writes_one_object_per_entry() -> Result<()> {
        let mut full = named(42 | (3 << 48), r"a\b.txt");
        full.si_created = Some(Filetime::from_raw(133_444_555_666_777_888));
        full.names[0].created = Filetime::from_raw(133_536_836_961_234_567);
        full.diagnostics = vec![Diagnostic {
            code: DiagCode::FixupMismatch,
            offset: 43_008,
        }];
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
            jsonl_with(&[named(5, "."), full, empty], None, None)?,
            concat!(
                r#"{"entry":5,"sequence":0,"in_use":true,"name":".","path":"\\","path_state":"resolved","baseline":null,"findings":[],"si_created":null,"fn_created":"1601-01-01T00:00:00.0000000Z","diagnostics":[],"source":"mft","usn":null,"reasons":[],"event_time":null}"#,
                "\n",
                r#"{"entry":42,"sequence":3,"in_use":true,"name":"a\\\\b.txt","path":"\\a\\\\b.txt","path_state":"resolved","baseline":null,"findings":[],"si_created":"2023-11-14T17:12:46.6777888Z","fn_created":"2024-02-29T12:34:56.1234567Z","diagnostics":["fixup_mismatch"],"source":"mft","usn":null,"reasons":[],"event_time":null}"#,
                "\n",
                r#"{"entry":43,"sequence":0,"in_use":false,"name":null,"path":null,"path_state":"unknown","baseline":null,"findings":[],"si_created":null,"fn_created":null,"diagnostics":["bad_signature","malformed"],"source":"mft","usn":null,"reasons":[],"event_time":null}"#,
                "\n",
            )
        );
        Ok(())
    }

    fn field(jsonl: &str, name: &str) -> Result<Vec<serde_json::Value>> {
        jsonl
            .lines()
            .map(|line| Ok(serde_json::from_str::<serde_json::Value>(line)?[name].take()))
            .collect()
    }

    #[test]
    fn jsonl_writes_baseline_status() -> Result<()> {
        let vwr = "\"DirectoryName\",\"Name\",\"FullName\"\n\"C:\\\",\"x.txt\",\"C:\\x.txt\"\n";
        let mut file = Vec::new();
        baseline::build(vwr.as_bytes(), &mut file)?;
        let baseline = baseline::Baseline::load(file)?;
        let mut root = named(5, ".");
        root.is_dir = true;

        let jsonl = jsonl_with(
            &[root, named(40, "X.TXT"), named(41, "y.txt")],
            Some(&baseline),
            None,
        )?;

        assert_eq!(
            field(&jsonl, "baseline")?,
            [serde_json::Value::Null, "standard".into(), "outside".into()]
        );
        Ok(())
    }

    fn exe_rules(test: &str, rules: &[(&str, &str)]) -> Result<sigma::Rules> {
        let dir = std::env::temp_dir().join(format!("report-jsonl-{test}-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        for (n, (id, level)) in rules.iter().enumerate() {
            std::fs::write(
                dir.join(format!("{n}.yml")),
                format!(
                    "title: T\nid: {id}\nlevel: {level}\nlogsource:\n  product: windows\n  category: file_event\ndetection:\n  sel:\n    TargetFilename|endswith: '.exe'\n  condition: sel\n"
                ),
            )?;
        }
        Ok(sigma::Rules::load(&dir)?)
    }

    #[test]
    fn jsonl_writes_findings_as_level_and_id_most_severe_first() -> Result<()> {
        let rules = exe_rules("findings", &[("rule-low", "low"), ("rule-high", "high")])?;
        let mut root = named(5, ".");
        root.is_dir = true;

        let jsonl = jsonl_with(&[root, named(40, "x.exe")], None, Some(&rules))?;

        assert_eq!(
            field(&jsonl, "findings")?,
            [
                serde_json::json!([]),
                serde_json::json!([
                    {"level": "high", "id": "rule-high"},
                    {"level": "low", "id": "rule-low"},
                ]),
            ]
        );
        Ok(())
    }

    #[test]
    fn jsonl_escapes_display_controls_but_keeps_formulas() -> Result<()> {
        let rules = exe_rules("escapes", &[("\"evil\\e[2J\"", "high"), ("\"=x\"", "low")])?;
        let mut root = named(5, ".");
        root.is_dir = true;
        let names = [
            ("=cmd|' /C calc'!A0.exe", "=cmd|' /C calc'!A0.exe"),
            ("a\u{1b}[31mb.exe", r"a\u{1B}[31mb.exe"),
            ("x\u{202E}fdp.exe", r"x\u{202E}fdp.exe"),
            ("q\"t.exe", "q\"t.exe"),
        ];
        let mut entries = vec![root];
        for (n, (name, _)) in (40..).zip(names) {
            entries.push(named(n, name));
        }

        let jsonl = jsonl_with(&entries, None, Some(&rules))?;

        let shown: Vec<serde_json::Value> = names.iter().map(|(_, text)| (*text).into()).collect();
        assert_eq!(field(&jsonl, "name")?[1..], shown);
        assert_eq!(
            field(&jsonl, "findings")?[1],
            serde_json::json!([
                {"level": "high", "id": r"evil\u{1B}[2J"},
                {"level": "low", "id": "=x"},
            ])
        );
        Ok(())
    }
}
