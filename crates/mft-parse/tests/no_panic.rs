//! No input makes `records` panic (`.claude/rules/rust.md`). The fuzz target in `fuzz/` goes deeper.

mod support;

use std::io::Cursor;

use proptest::prelude::*;
use support::{file_name_with, record, resident, standard_information, u16s};

fn drain(mft: Vec<u8>) {
    if let Ok(records) = mft_parse::records(Cursor::new(mft)) {
        for item in records {
            let _ = item;
        }
    }
}

fn valid_mft(size: usize) -> Vec<u8> {
    let attrs = [
        standard_information(),
        file_name_with(5 | (5 << 48), &u16s("file.txt"), 1, 133_000_000_000_000_000),
        resident(0x80, &u16s("Zone.Identifier"), b"ZoneId=3", 2),
    ];
    let mut mft = record(size, true, 0, &attrs);
    mft.extend(record(size, true, 1, &attrs));
    mft
}

proptest! {
    /// Any bytes at all, as a whole `$MFT`.
    #[test]
    fn arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..9000)) {
        drain(bytes);
    }

    /// Valid records with byte overwrites (headers, lengths, offsets, fixups) and a cut-off end.
    #[test]
    fn corrupted_valid_records(
        size in prop_oneof![Just(1024usize), Just(4096usize)],
        edits in proptest::collection::vec((any::<u16>(), any::<u8>()), 1..32),
        keep in any::<u16>(),
    ) {
        let mut mft = valid_mft(size);
        for (at, byte) in edits {
            let i = usize::from(at) % mft.len();
            mft[i] = byte;
        }
        mft.truncate(usize::from(keep) % (mft.len() + 1) + mft.len() / 2);
        drain(mft);
    }
}
