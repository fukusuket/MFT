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
                    Resolution::Unknown => None,
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
}
