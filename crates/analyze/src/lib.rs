//! Pipeline from parsed `$MFT` entries to report rows. Phase 1: name choice only.

use mft_parse::{FileName, Namespace};

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
    use ntfs_types::{FileRef, Filetime, NtfsName};

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
}
