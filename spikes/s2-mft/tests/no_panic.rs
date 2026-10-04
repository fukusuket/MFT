//! Spike S2, option (b): stable-Rust proptest instead of cargo-fuzz.
//! Every parse result is fully walked, including the `utf16-simd` paths
//! (`to_utf8_string`, `Display`, serde) that touch attacker-controlled names.
//! Case count: `PROPTEST_CASES` (default 256); see README for the long run.

use mft::MftEntry;
use mft::attribute::MftAttributeContent;
use proptest::prelude::*;
use s2_mft::{data, file_name, record, standard_information, u16s};

fn walk(buf: Vec<u8>) {
    let Ok(entry) = MftEntry::from_buffer(buf, 0) else {
        return;
    };
    let _ = entry
        .find_best_name_attribute()
        .map(|n| n.name.to_utf8_string());
    for attr in entry.iter_attributes().flatten() {
        let _ = attr.header.name.to_utf8_string();
        let _ = serde_json::to_string(&attr);
        if let MftAttributeContent::AttrX30(f) = &attr.data {
            let _ = format!("{}", f.name);
            let _ = f.name.eq_ignore_ascii_case("x");
        }
    }
    let _ = serde_json::to_string(&entry);
}

fn valid(size: usize, name: &[u16]) -> Vec<u8> {
    record(
        size,
        true,
        1,
        &[
            standard_information(),
            file_name(5, name),
            data(&u16s("Zone.Identifier"), b"ZoneId=3", 2),
        ],
    )
}

fn size() -> impl Strategy<Value = usize> {
    prop_oneof![Just(1024usize), Just(4096usize)]
}

proptest! {
    /// Any bytes at all.
    #[test]
    fn arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..4200)) {
        walk(bytes);
    }

    /// Valid records with random byte overwrites (headers, lengths, offsets, fixups).
    #[test]
    fn corrupted_valid_records(size in size(), edits in proptest::collection::vec((any::<u16>(), any::<u8>()), 1..16)) {
        let mut buf = valid(size, &u16s("file.txt"));
        for (at, byte) in edits {
            let i = usize::from(at) % buf.len();
            buf[i] = byte;
        }
        walk(buf);
    }

    /// Valid records whose name is any UTF-16 sequence: drives `utf16-simd`.
    #[test]
    fn arbitrary_names(size in size(), name in proptest::collection::vec(any::<u16>(), 0..255)) {
        walk(valid(size, &name));
    }
}
