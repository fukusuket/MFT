//! Pipeline from parsed `$MFT` entries to report rows.

use mft_parse::Entry;
use resolve::{Resolution, Resolver};

/// One report row: an entry and where it sits in the tree.
#[derive(Debug)]
pub struct Row<'a> {
    pub entry: &'a Entry,
    pub resolution: Resolution<'a>,
}

/// One row per entry, in entry order. Lazy, so only one path is held at a time.
pub fn rows(entries: &[Entry]) -> impl Iterator<Item = Row<'_>> {
    let resolver = Resolver::new(entries);
    entries.iter().map(move |entry| Row {
        entry,
        resolution: resolver.path(entry),
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

        let shown: Vec<(u64, Option<usize>)> = rows(&entries)
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
}
