//! Paths for `$MFT` entries from `$FILE_NAME` parent references (MFT only in Phase 1).

use mft_parse::{Entry, FileName, Namespace};
use ntfs_types::NtfsName;

/// The root directory's MFT entry number.
const ROOT: u64 = 5;

/// Longest path Windows can create, in UTF-16 units. Longer chains come only from corrupt or
/// crafted records, and capping them bounds each walk.
const MAX_PATH_UNITS: usize = 32_767;

/// Where an entry sits in the directory tree.
#[derive(Debug)]
pub enum Resolution<'a> {
    /// Names from the root down; the root itself has none.
    Resolved(Vec<&'a NtfsName>),
    /// The chain of parents is broken (missing, deleted or reused); no path is invented.
    Unknown,
}

impl Resolution<'_> {
    /// `\` plus the escaped names joined by `\` (a `\` inside a name shows as `\\`); the root
    /// is `\`. `None` if unknown. No drive letter: a `$MFT` doesn't record one.
    pub fn path_text(&self) -> Option<String> {
        let Resolution::Resolved(segments) = self else {
            return None;
        };
        if segments.is_empty() {
            return Some("\\".to_string());
        }
        let mut path = String::new();
        for segment in segments {
            path.push('\\');
            path.push_str(&segment.to_string());
        }
        Some(path)
    }
}

/// Builds paths from `$FILE_NAME` parent references, checking sequence numbers.
#[derive(Debug)]
pub struct Resolver<'a> {
    entries: &'a [Entry],
    /// Position in `entries` by MFT entry number.
    by_number: Vec<Option<usize>>,
    /// Per position: does the chain of parents reach the root?
    reaches_root: Vec<bool>,
}

impl<'a> Resolver<'a> {
    pub fn new(entries: &'a [Entry]) -> Self {
        let mut by_number = Vec::new();
        for (position, entry) in entries.iter().enumerate() {
            let Ok(number) = usize::try_from(entry.file_ref.entry()) else {
                continue;
            };
            if by_number.len() <= number {
                by_number.resize(number + 1, None);
            }
            by_number[number] = Some(position);
        }
        let mut resolver = Self {
            entries,
            by_number,
            reaches_root: Vec::new(),
        };
        resolver.reaches_root = resolver.reachability();
        resolver
    }

    pub fn path(&self, entry: &'a Entry) -> Resolution<'a> {
        if !entry.in_use && entry.file_ref.entry() == ROOT {
            return Resolution::Unknown; // the root slot holds no usable record
        }
        let mut segments = Vec::new();
        let mut units = 0usize;
        let mut current = entry;
        while current.file_ref.entry() != ROOT {
            let Some((name, parent)) = self.step(current) else {
                return Resolution::Unknown;
            };
            units += 1 + name.name.units().len(); // `\` + name
            if !self.reaches_root[parent] || units > MAX_PATH_UNITS {
                return Resolution::Unknown;
            }
            segments.push(&name.name);
            current = &self.entries[parent];
        }
        segments.reverse();
        Resolution::Resolved(segments)
    }

    /// The chosen name and the position of its parent, if that parent is the same, live record.
    fn step(&self, entry: &'a Entry) -> Option<(&'a FileName, usize)> {
        let name = chosen_name(&entry.names)?;
        let position = (*self
            .by_number
            .get(usize::try_from(name.parent.entry()).ok()?)?)?;
        let parent = self.entries.get(position)?;
        (parent.in_use && parent.file_ref.sequence() == name.parent.sequence())
            .then_some((name, position))
    }

    /// One pass over all entries: each is visited once, so cycles cost O(n), not O(n²).
    fn reachability(&self) -> Vec<bool> {
        #[derive(Clone, Copy, PartialEq)]
        enum State {
            New,
            OnWalk,
            Done(bool),
        }
        let mut state = vec![State::New; self.entries.len()];
        let mut walk = Vec::new();
        for start in 0..self.entries.len() {
            let mut current = start;
            let reaches = loop {
                match state[current] {
                    State::Done(reaches) => break reaches,
                    State::OnWalk => break false, // a cycle
                    State::New => {}
                }
                state[current] = State::OnWalk;
                walk.push(current);
                if self.entries[current].file_ref.entry() == ROOT {
                    break true;
                }
                match self.step(&self.entries[current]) {
                    Some((_, parent)) => current = parent,
                    None => break false,
                }
            };
            for visited in walk.drain(..) {
                state[visited] = State::Done(reaches);
            }
        }
        state.into_iter().map(|s| s == State::Done(true)).collect()
    }
}

/// The name shown for an entry, and whose `$FN` created time is reported (ADR 0011):
/// Win32 or Win32+DOS first, then POSIX, then DOS; ties go to the earlier attribute.
pub fn chosen_name(names: &[FileName]) -> Option<&FileName> {
    names.iter().min_by_key(|n| match n.namespace {
        Namespace::Win32 | Namespace::Win32AndDos => 0,
        Namespace::Posix => 1,
        Namespace::Dos => 2,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntfs_types::{FileRef, Filetime};

    fn name(text: &str, namespace: Namespace) -> FileName {
        FileName {
            name: NtfsName::from_units(&text.encode_utf16().collect::<Vec<_>>()),
            parent: FileRef::from_raw(5),
            namespace,
            created: Filetime::from_raw(0),
        }
    }

    fn chosen(names: &[FileName]) -> Option<String> {
        chosen_name(names).map(|n| n.name.to_string())
    }

    #[test]
    fn prefers_win32_then_posix_then_dos_and_keeps_attribute_order_on_ties() {
        use Namespace::*;
        let cases: [(&[FileName], Option<&str>); 6] = [
            (&[], None),
            (&[name("DOS~1", Dos), name("long", Win32)], Some("long")),
            (
                &[name("DOS~1", Dos), name("both", Win32AndDos)],
                Some("both"),
            ),
            (&[name("DOS~1", Dos), name("posix", Posix)], Some("posix")),
            (&[name("posix", Posix), name("win", Win32)], Some("win")),
            (
                &[name("first", Win32), name("second", Win32AndDos)],
                Some("first"),
            ),
        ];
        for (names, expected) in cases {
            assert_eq!(chosen(names).as_deref(), expected);
        }
    }

    /// An in-use entry with one Win32 name per `(name, parent entry, parent sequence)`.
    fn entry(number: u64, sequence: u16, names: &[(&str, u64, u16)]) -> Entry {
        Entry {
            file_ref: FileRef::from_raw(number | (u64::from(sequence) << 48)),
            in_use: true,
            is_dir: false,
            base: None,
            si_created: None,
            names: names
                .iter()
                .map(|&(n, parent, parent_seq)| FileName {
                    parent: FileRef::from_raw(parent | (u64::from(parent_seq) << 48)),
                    ..name(n, Namespace::Win32)
                })
                .collect(),
            diagnostics: vec![],
        }
    }

    fn shown(resolution: &Resolution<'_>) -> Option<Vec<String>> {
        match resolution {
            Resolution::Resolved(segments) => {
                Some(segments.iter().map(|s| s.to_string()).collect())
            }
            Resolution::Unknown => None,
        }
    }

    #[test]
    fn root_resolves_to_no_segments() {
        let entries = [entry(ROOT, 5, &[(".", ROOT, 5)])];

        let resolver = Resolver::new(&entries);

        assert_eq!(shown(&resolver.path(&entries[0])), Some(vec![]));
    }

    #[test]
    fn file_in_the_root_has_one_segment() {
        let entries = [
            entry(ROOT, 5, &[(".", ROOT, 5)]),
            entry(40, 2, &[("a.txt", ROOT, 5)]),
        ];

        let resolver = Resolver::new(&entries);

        assert_eq!(
            shown(&resolver.path(&entries[1])),
            Some(vec!["a.txt".to_string()])
        );
    }

    #[test]
    fn nested_directories_give_segments_from_the_root_down() {
        let entries = [
            entry(ROOT, 5, &[(".", ROOT, 5)]),
            entry(30, 1, &[("Windows", ROOT, 5)]),
            entry(31, 4, &[("System32", 30, 1)]),
            entry(99, 7, &[("cmd.exe", 31, 4)]),
        ];

        let resolver = Resolver::new(&entries);

        assert_eq!(
            shown(&resolver.path(&entries[3])),
            Some(vec![
                "Windows".to_string(),
                "System32".to_string(),
                "cmd.exe".to_string()
            ])
        );
    }

    #[test]
    fn reused_parent_record_is_unknown() {
        let entries = [
            entry(ROOT, 5, &[(".", ROOT, 5)]),
            entry(30, 2, &[("new dir", ROOT, 5)]),
            entry(99, 1, &[("old file", 30, 1)]),
        ];

        let resolver = Resolver::new(&entries);

        assert_eq!(shown(&resolver.path(&entries[2])), None);
    }

    #[test]
    fn missing_or_nameless_parent_is_unknown() {
        let entries = [
            entry(ROOT, 5, &[(".", ROOT, 5)]),
            entry(30, 0, &[]), // e.g. a BAAD record: position only
            entry(98, 1, &[("orphan", 77, 1)]),
            entry(99, 1, &[("under bad", 30, 0)]),
        ];

        let resolver = Resolver::new(&entries);

        assert_eq!(shown(&resolver.path(&entries[2])), None);
        assert_eq!(shown(&resolver.path(&entries[3])), None);
    }

    #[test]
    fn deleted_parent_directory_is_unknown() {
        let mut deleted_dir = entry(30, 3, &[("gone", ROOT, 5)]);
        deleted_dir.in_use = false;
        let entries = [
            entry(ROOT, 5, &[(".", ROOT, 5)]),
            deleted_dir,
            entry(99, 1, &[("file", 30, 3)]),
        ];

        let resolver = Resolver::new(&entries);

        assert_eq!(shown(&resolver.path(&entries[2])), None);
    }

    #[test]
    fn deleted_file_in_a_live_directory_keeps_its_path() {
        let mut deleted = entry(99, 2, &[("deleted.exe", ROOT, 5)]);
        deleted.in_use = false;
        let entries = [entry(ROOT, 5, &[(".", ROOT, 5)]), deleted];

        let resolver = Resolver::new(&entries);

        assert_eq!(
            shown(&resolver.path(&entries[1])),
            Some(vec!["deleted.exe".to_string()])
        );
    }

    #[test]
    fn entry_without_names_is_unknown() {
        let entries = [entry(ROOT, 5, &[(".", ROOT, 5)]), entry(99, 1, &[])];

        let resolver = Resolver::new(&entries);

        assert_eq!(shown(&resolver.path(&entries[1])), None);
    }

    #[test]
    fn parent_cycles_are_unknown_and_terminate() {
        let entries = [
            entry(ROOT, 5, &[(".", ROOT, 5)]),
            entry(30, 1, &[("a", 31, 1)]),
            entry(31, 1, &[("b", 30, 1)]),
            entry(32, 1, &[("self", 32, 1)]),
        ];

        let resolver = Resolver::new(&entries);

        assert_eq!(shown(&resolver.path(&entries[1])), None);
        assert_eq!(shown(&resolver.path(&entries[3])), None);
    }

    #[test]
    fn every_segment_uses_the_chosen_name() {
        let mut dir = entry(30, 1, &[]);
        dir.names = vec![
            FileName {
                parent: FileRef::from_raw(ROOT | (5 << 48)),
                ..name("PROGRA~1", Namespace::Dos)
            },
            FileName {
                parent: FileRef::from_raw(ROOT | (5 << 48)),
                ..name("Program Files", Namespace::Win32)
            },
        ];
        let entries = [
            entry(ROOT, 5, &[(".", ROOT, 5)]),
            dir,
            entry(99, 1, &[("app.exe", 30, 1)]),
        ];

        let resolver = Resolver::new(&entries);

        assert_eq!(
            shown(&resolver.path(&entries[2])),
            Some(vec!["Program Files".to_string(), "app.exe".to_string()])
        );
    }

    #[test]
    fn many_entries_leading_into_a_cycle_resolve_in_linear_time() {
        let n: u64 = 200_000;
        let mut entries = vec![
            entry(ROOT, 5, &[(".", ROOT, 5)]),
            entry(10, 1, &[("a", 11, 1)]),
            entry(11, 1, &[("b", 10, 1)]),
        ];
        entries.extend((100..100 + n).map(|e| entry(e, 1, &[("f", 10, 1)])));

        let resolver = Resolver::new(&entries);

        let unknown = entries
            .iter()
            .filter(|e| shown(&resolver.path(e)).is_none())
            .count();
        assert_eq!(unknown, entries.len() - 1);
    }

    #[test]
    fn paths_longer_than_the_windows_maximum_are_unknown() {
        // Each level adds `\d`: 2 units. Level 16_383 is 32_766 units, level 16_384 is 32_768.
        let levels: u64 = 16_384;
        let mut entries = vec![entry(ROOT, 5, &[(".", ROOT, 5)])];
        let mut parent = ROOT;
        let mut parent_seq = 5;
        for e in 100..100 + levels {
            entries.push(entry(e, 1, &[("d", parent, parent_seq)]));
            parent = e;
            parent_seq = 1;
        }

        let resolver = Resolver::new(&entries);

        let depth = |position: usize| shown(&resolver.path(&entries[position])).map(|s| s.len());
        assert_eq!(depth(16_383), Some(16_383));
        assert_eq!(depth(16_384), None);
    }

    #[test]
    fn unusable_root_record_is_unknown() {
        let mut bad_root = entry(ROOT, 0, &[]); // e.g. a BAAD record at position 5
        bad_root.in_use = false;
        let entries = [bad_root];

        let resolver = Resolver::new(&entries);

        assert_eq!(shown(&resolver.path(&entries[0])), None);
    }

    use proptest::prelude::*;

    /// Entries numbered by position (as the parser numbers them), with random parents.
    fn arbitrary_entries() -> impl Strategy<Value = Vec<Entry>> {
        let one = (
            0u16..3,
            any::<bool>(),
            proptest::collection::vec((0u64..14, 0u16..3), 0..3),
        );
        proptest::collection::vec(one, 0..24).prop_map(|raw| {
            raw.into_iter()
                .zip(0u64..)
                .map(|((seq, in_use, parents), number)| {
                    let names: Vec<(&str, u64, u16)> =
                        parents.iter().map(|&(p, s)| ("n", p, s)).collect();
                    Entry {
                        in_use,
                        ..entry(number, seq, &names)
                    }
                })
                .collect()
        })
    }

    proptest! {
        /// Any parent graph (cycles, dangling refs, stale sequences): no panic, no hang, and every
        /// resolved path is its parent's resolved path plus the entry's chosen name.
        #[test]
        fn arbitrary_parent_graphs(entries in arbitrary_entries()) {
            let resolver = Resolver::new(&entries);
            for e in &entries {
                let Resolution::Resolved(segments) = resolver.path(e) else { continue };
                if e.file_ref.entry() == ROOT {
                    prop_assert!(segments.is_empty());
                    continue;
                }
                let name = chosen_name(&e.names);
                prop_assert!(name.is_some());
                let Some(name) = name else { continue };
                prop_assert_eq!(segments.last().map(|s| s.units()), Some(name.name.units()));
                let parent = entries.iter().find(|p| p.file_ref.entry() == name.parent.entry());
                prop_assert!(parent.is_some());
                let Some(parent) = parent else { continue };
                prop_assert!(parent.in_use && parent.file_ref.sequence() == name.parent.sequence());
                let parent_segments = match resolver.path(parent) {
                    Resolution::Resolved(p) => Some(p.iter().map(|s| s.units().to_vec()).collect::<Vec<_>>()),
                    Resolution::Unknown => None,
                };
                let expected: Vec<Vec<u16>> =
                    segments[..segments.len() - 1].iter().map(|s| s.units().to_vec()).collect();
                prop_assert_eq!(parent_segments, Some(expected));
            }
        }
    }
}
