//! Report writers. Phase 1: CSV, one row per `$MFT` record.

mod time;

use std::fmt::Write as _;
use std::io::Write;

use mft_parse::{DiagCode, Entry};
use ntfs_types::NtfsName;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("writing CSV: {0}")]
    Csv(#[from] csv::Error),
}

const HEADER: [&str; 7] = [
    "entry",
    "sequence",
    "in_use",
    "name",
    "si_created",
    "fn_created",
    "diagnostics",
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

    pub fn write(&mut self, entry: &Entry) -> Result<(), Error> {
        let name = analyze::chosen_name(&entry.names);
        let diagnostics: Vec<&str> = entry.diagnostics.iter().map(|d| code(d.code)).collect();
        self.csv.write_record([
            entry.file_ref.entry().to_string(),
            entry.file_ref.sequence().to_string(),
            entry.in_use.to_string(),
            name.map(|n| cell_text(&n.name)).unwrap_or_default(),
            entry.si_created.map(time::iso8601).unwrap_or_default(),
            name.map(|n| time::iso8601(n.created)).unwrap_or_default(),
            diagnostics.join(";"),
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

/// A file name as a CSV cell (ADR 0002 #4): control characters escaped as `\u{XX}`, and a
/// leading `=`, `+`, `-` or `@` prefixed with `'` so spreadsheets don't run it as a formula.
fn cell_text(name: &NtfsName) -> String {
    let mut out = String::new();
    for c in name.to_string().chars() {
        if c.is_control() {
            let _ = write!(out, "\\u{{{:02X}}}", u32::from(c)); // writing to a String can't fail
        } else {
            out.push(c);
        }
    }
    if out.starts_with(['=', '+', '-', '@']) {
        out.insert(0, '\'');
    }
    out
}

fn code(code: DiagCode) -> &'static str {
    match code {
        DiagCode::BadSignature => "bad_signature",
        DiagCode::FixupMismatch => "fixup_mismatch",
        DiagCode::Malformed => "malformed",
        DiagCode::Truncated => "truncated",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mft_parse::{Diagnostic, FileName, Namespace};
    use ntfs_types::{FileRef, Filetime};

    fn units(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    fn csv_of(entries: &[Entry]) -> Result<String, Box<dyn std::error::Error>> {
        let mut w = CsvWriter::new(Vec::new())?;
        for e in entries {
            w.write(e)?;
        }
        Ok(String::from_utf8(w.finish()?)?)
    }

    #[test]
    fn writes_header_and_one_row_per_entry() -> Result<(), Box<dyn std::error::Error>> {
        let full = Entry {
            file_ref: FileRef::from_raw(42 | (3 << 48)),
            in_use: true,
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
        let empty = Entry {
            file_ref: FileRef::from_raw(43),
            in_use: false,
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
            csv_of(&[full, empty])?,
            "entry,sequence,in_use,name,si_created,fn_created,diagnostics\n\
             42,3,true,a\\\\b.txt,2023-11-14T17:12:46.6777888Z,2024-02-29T12:34:56.1234567Z,fixup_mismatch\n\
             43,0,false,,,,bad_signature;malformed\n"
        );
        Ok(())
    }

    fn named(name: &str) -> Entry {
        Entry {
            file_ref: FileRef::from_raw(0),
            in_use: true,
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
            ("a,b", r#""a,b""#),
            ("a\"b", r#""a""b""#),
        ];
        for (name, cell) in cases {
            let csv = csv_of(&[named(name)])?;
            let row = csv.lines().nth(1).unwrap_or_default();
            assert_eq!(
                row,
                format!("0,0,true,{cell},,1601-01-01T00:00:00.0000000Z,"),
                "name {name:?}"
            );
        }
        Ok(())
    }
}
