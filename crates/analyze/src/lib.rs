//! Pipeline from parsed `$MFT` entries to report rows.

use baseline::{Baseline, Status};
use mft_parse::Entry;
use resolve::{Resolution, Resolver};
use sigma::{Finding, Rules};

/// One report row: an entry and where it sits in the tree.
#[derive(Debug)]
pub struct Row<'a> {
    pub entry: &'a Entry,
    pub resolution: Resolution<'a>,
    /// For files with a resolved path, when a baseline is given; `None` otherwise.
    pub baseline: Option<Status>,
    /// Matched rules, most severe first (empty without rules).
    pub findings: Vec<Finding>,
}

/// One row per entry, in entry order. Lazy, so only one path is held at a time.
pub fn rows<'a>(
    entries: &'a [Entry],
    baseline: Option<&'a Baseline>,
    rules: Option<&'a Rules>,
) -> impl Iterator<Item = Row<'a>> {
    let resolver = Resolver::new(entries);
    entries.iter().map(move |entry| {
        let resolution = resolver.path(entry);
        let status = match (&resolution, baseline) {
            (Resolution::Resolved(segments), Some(baseline))
                if !entry.is_dir && !segments.is_empty() =>
            {
                let units: Vec<&[u16]> = segments.iter().map(|s| s.units()).collect();
                Some(baseline.status(&units))
            }
            _ => None,
        };
        let findings = match (rules, detect::file_event(entry, &resolution, status)) {
            (Some(rules), Some(event)) => rules.evaluate(&event),
            _ => Vec::new(),
        };
        Row {
            entry,
            resolution,
            baseline: status,
            findings,
        }
    })
}

/// One USN row: a record from `$J` and the directory its parent reference pointed to when the
/// record was written (Rewind).
#[derive(Debug)]
pub struct UsnRow<'a> {
    pub record: &'a usn_parse::Record,
    pub directory: Resolution<'a>,
}

/// One row per `$J` item, in `$J` order.
pub fn usn_rows<'a>(
    entries: &'a [Entry],
    records: &'a [usn_parse::Record],
) -> impl Iterator<Item = UsnRow<'a>> {
    let directories = Resolver::new(entries).rewind(records);
    records
        .iter()
        .zip(directories)
        .map(|(record, directory)| UsnRow { record, directory })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mft_parse::{FileName, Namespace};
    use ntfs_types::{FileRef, Filetime, NtfsName};

    fn entry(number: u64, parent: u64, name: &str) -> Entry {
        Entry {
            file_ref: FileRef::from_raw(number | (1 << 48)),
            in_use: true,
            is_dir: false,
            base: None,
            si_created: None,
            names: vec![FileName {
                name: NtfsName::from_units(&name.encode_utf16().collect::<Vec<_>>()),
                parent: FileRef::from_raw(parent | (1 << 48)),
                namespace: Namespace::Win32,
                created: Filetime::from_raw(0),
            }],
            diagnostics: vec![],
        }
    }

    #[test]
    fn pairs_each_entry_with_its_resolution_in_entry_order() {
        let entries = [
            entry(5, 5, "."),
            entry(40, 5, "a.txt"),
            entry(41, 77, "orphan"),
        ];

        let shown: Vec<(u64, Option<usize>)> = rows(&entries, None, None)
            .map(|r| {
                let depth = match &r.resolution {
                    Resolution::Resolved(segments) => Some(segments.len()),
                    Resolution::Inferred(_) | Resolution::Unknown => None,
                };
                (r.entry.file_ref.entry(), depth)
            })
            .collect();

        assert_eq!(shown, [(5, Some(0)), (40, Some(1)), (41, None)]);
    }

    #[test]
    fn files_with_a_resolved_path_get_a_baseline_status() -> Result<(), baseline::Error> {
        let vwr = "\"DirectoryName\",\"Name\",\"FullName\"\n\"C:\\\",\"a.txt\",\"C:\\a.txt\"\n";
        let mut file = Vec::new();
        baseline::build(vwr.as_bytes(), &mut file)?;
        let baseline = Baseline::load(file)?;
        let mut dir = entry(41, 5, "a.txt");
        dir.is_dir = true;
        let entries = [
            entry(5, 5, "."),
            entry(40, 5, "a.txt"),
            dir,
            entry(42, 5, "b.txt"),
            entry(43, 77, "a.txt"),
        ];

        let with: Vec<Option<Status>> = rows(&entries, Some(&baseline), None)
            .map(|r| r.baseline)
            .collect();
        let without: Vec<Option<Status>> = rows(&entries, None, None).map(|r| r.baseline).collect();

        use Status::*;
        assert_eq!(with, [None, Some(Standard), None, Some(Outside), None]);
        assert_eq!(without, [None; 5]);
        Ok(())
    }

    #[test]
    fn rows_carry_findings_when_rules_are_given() -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!("analyze-rules-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            dir.join("exe.yml"),
            "title: Exe\nid: rule-exe\nlevel: low\nlogsource:\n  product: windows\n  category: file_event\ndetection:\n  sel:\n    TargetFilename|endswith: '.exe'\n  condition: sel\n",
        )?;
        let rules = Rules::load(&dir)?;
        let entries = [
            entry(5, 5, "."),
            entry(40, 5, "a.exe"),
            entry(41, 5, "b.txt"),
        ];

        let with: Vec<Vec<String>> = rows(&entries, None, Some(&rules))
            .map(|r| r.findings.into_iter().map(|f| f.id).collect())
            .collect();
        let without = rows(&entries, None, None)
            .filter(|r| !r.findings.is_empty())
            .count();

        assert_eq!(with, [vec![], vec!["rule-exe".to_string()], vec![]]);
        assert_eq!(without, 0);
        Ok(())
    }

    fn usn_event(name: &str, parent: u64) -> usn_parse::Record {
        usn_parse::Record::Event(usn_parse::UsnEvent {
            offset: 0,
            usn: 0,
            file: FileRef::from_raw(60 | (1 << 48)),
            parent: FileRef::from_raw(parent | (1 << 48)),
            time: Filetime::from_raw(0),
            reason: 0,
            attributes: 0,
            name: NtfsName::from_units(&name.encode_utf16().collect::<Vec<_>>()),
        })
    }

    #[test]
    fn usn_rows_pair_each_record_with_its_parent_directory_in_order() {
        let mut root = entry(5, 5, ".");
        root.is_dir = true;
        let mut users = entry(6, 5, "Users");
        users.is_dir = true;
        let entries = [root, users];
        let diagnostic = usn_parse::Record::Diagnostic(usn_parse::Diagnostic {
            code: usn_parse::DiagCode::Malformed,
            offset: 8,
        });
        let records = [usn_event("a.txt", 6), usn_event("b.txt", 77), diagnostic];

        let shown: Vec<Option<String>> = usn_rows(&entries, &records)
            .map(|r| r.directory.path_text())
            .collect();

        assert_eq!(shown, [Some("\\Users".to_string()), None, None]);
    }

    #[test]
    fn usn_rows_use_the_path_at_the_time_of_each_event() {
        let mut root = entry(5, 5, ".");
        root.is_dir = true;
        let mut renamed = entry(6, 5, "New");
        renamed.is_dir = true;
        let entries = [root, renamed];
        let records = [
            usn_event("a.txt", 6),    // while \New was still \Old
            usn_dir_rename(6, "Old"), // RENAME_OLD_NAME
            usn_dir_rename(6, "New"), // RENAME_NEW_NAME
            usn_event("b.txt", 6),
        ];

        let shown: Vec<(Option<String>, bool)> = usn_rows(&entries, &records)
            .map(|r| {
                (
                    r.directory.path_text(),
                    matches!(r.directory, Resolution::Inferred(_)),
                )
            })
            .collect();

        assert_eq!(
            shown,
            [
                (Some("\\Old".to_string()), true),
                (Some("\\".to_string()), false),
                (Some("\\".to_string()), false),
                (Some("\\New".to_string()), false),
            ]
        );
    }

    fn usn_dir_rename(dir: u64, name: &str) -> usn_parse::Record {
        let usn_parse::Record::Event(mut event) = usn_event(name, 5) else {
            return usn_event(name, 5);
        };
        event.file = FileRef::from_raw(dir | (1 << 48));
        event.attributes = 0x10; // FILE_ATTRIBUTE_DIRECTORY
        usn_parse::Record::Event(event)
    }
}
