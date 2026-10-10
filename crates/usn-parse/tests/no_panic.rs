//! No input makes `records` panic or loop forever (`.claude/rules/rust.md`). The fuzz target
//! in `fuzz/` goes deeper.

mod support;

use std::io::Cursor;

use proptest::prelude::*;
use support::{Fields, other_version, u16s, v2, v3};

/// Drains the stream; every item must start before the end of the input.
fn drain(j: Vec<u8>) {
    let len = u64::try_from(j.len()).unwrap_or(u64::MAX);
    for item in usn_parse::records(Cursor::new(j)) {
        let offset = match item {
            Ok(usn_parse::Record::Event(e)) => e.offset,
            Ok(usn_parse::Record::Diagnostic(d)) => d.offset,
            Err(_) => break,
        };
        assert!(offset < len, "offset {offset} past the input ({len})");
    }
}

fn valid_journal() -> Vec<u8> {
    let f = Fields {
        name: u16s("Zone.Identifier"),
        ..Fields::default()
    };
    let mut j = vec![0u8; 64];
    j.extend(v2(&f));
    j.extend(v3(&f, 0, 0));
    j.extend(other_version(4, 0x50));
    j.extend(v2(&f));
    j
}

proptest! {
    /// Any bytes at all, as a whole `$J`.
    #[test]
    fn arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..9000)) {
        drain(bytes);
    }

    /// Valid records with byte overwrites (lengths, versions, name fields) and a cut-off end.
    #[test]
    fn corrupted_valid_records(
        edits in proptest::collection::vec((any::<u16>(), any::<u8>()), 1..32),
        keep in any::<u16>(),
    ) {
        let mut j = valid_journal();
        for (at, byte) in edits {
            let i = usize::from(at) % j.len();
            j[i] = byte;
        }
        j.truncate(usize::from(keep) % (j.len() + 1));
        drain(j);
    }
}
