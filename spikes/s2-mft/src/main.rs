//! Phase 0 spike S2: does the `mft` crate (master @ 18b6c05) meet our needs?
//! Usage: `cargo run --release` (see README.md). Exit code 1 if any check fails.

use mft::MftEntry;
use mft::attribute::{MftAttributeContent, MftAttributeType};
use s2_mft::{data, file_name, record, standard_information, u16s};

fn units_of(le: &[u8]) -> Vec<u16> {
    le.as_chunks::<2>()
        .0
        .iter()
        .map(|&c| u16::from_le_bytes(c))
        .collect()
}

struct Report(bool);
impl Report {
    fn check(&mut self, what: &str, ok: bool, detail: String) {
        println!("[{}] {what}: {detail}", if ok { "PASS" } else { "FAIL" });
        self.0 &= ok;
    }
}

fn main() {
    let mut rep = Report(true);
    let name = [0x0061, 0xD800, 0x0062]; // "a" + unpaired high surrogate + "b"
    let ads = u16s("Zone.Identifier");

    for size in [1024usize, 4096] {
        let rec = record(
            size,
            true,
            42,
            &[
                standard_information(),
                file_name(5, &name),
                data(&[], b"hello", 2),
                data(&ads, b"[ZoneTransfer]\r\nZoneId=3\r\n", 3),
            ],
        );
        let entry = match MftEntry::from_buffer(rec, 42) {
            Ok(e) => e,
            Err(e) => {
                rep.check(&format!("{size}-byte record parses"), false, e.to_string());
                continue;
            }
        };
        rep.check(
            &format!("{size}-byte record parses"),
            true,
            format!("valid_fixup={:?}", entry.valid_fixup),
        );

        let mut got_name = None;
        let mut main_stream = None;
        let mut ads_stream = None;
        for attr in entry.iter_attributes().flatten() {
            match (&attr.header.type_code, &attr.data) {
                (MftAttributeType::FileName, MftAttributeContent::AttrX30(f)) => {
                    got_name = Some((units_of(f.name.as_utf16le_bytes()), f.name.to_utf8_string()));
                }
                (MftAttributeType::DATA, MftAttributeContent::AttrX80(d)) => {
                    let stream = units_of(attr.header.name.as_utf16le_bytes());
                    if stream.is_empty() {
                        main_stream = Some(d.data().to_vec());
                    } else {
                        ads_stream = Some((stream, d.data().to_vec()));
                    }
                }
                _ => {}
            }
        }
        let (units, lossy) = got_name.unwrap_or_default();
        rep.check(
            &format!("{size}: unpaired surrogate survives (raw units)"),
            units == name,
            format!("units={units:04X?} to_utf8_string={lossy:?}"),
        );
        rep.check(
            &format!("{size}: resident main $DATA"),
            main_stream.as_deref() == Some(b"hello".as_slice()),
            format!("{:?}", main_stream.as_deref().map(String::from_utf8_lossy)),
        );
        rep.check(
            &format!("{size}: ADS name and content"),
            ads_stream
                .as_ref()
                .is_some_and(|(n, c)| *n == ads && c.starts_with(b"[ZoneTransfer]")),
            format!(
                "{:?}",
                ads_stream.map(|(n, _)| String::from_utf16_lossy(&n))
            ),
        );
    }

    let deleted = record(
        1024,
        false,
        7,
        &[standard_information(), file_name(5, &u16s("gone.exe"))],
    );
    match MftEntry::from_buffer(deleted, 7) {
        Ok(e) => {
            let names: Vec<String> = e
                .iter_attributes()
                .flatten()
                .filter_map(|a| match a.data {
                    MftAttributeContent::AttrX30(f) => Some(f.name.to_utf8_string()),
                    _ => None,
                })
                .collect();
            rep.check(
                "deleted entry reported",
                !e.is_allocated() && names == ["gone.exe"],
                format!("is_allocated={} names={names:?}", e.is_allocated()),
            );
        }
        Err(err) => rep.check("deleted entry reported", false, err.to_string()),
    }

    // Torn write: corrupt one sector's fixup bytes; parsing must still report it, not panic.
    let mut torn = record(
        4096,
        true,
        9,
        &[standard_information(), file_name(5, &u16s("t.txt"))],
    );
    torn[1022] ^= 0xFF;
    match MftEntry::from_buffer(torn, 9) {
        Ok(e) => rep.check(
            "torn sector detected",
            e.valid_fixup == Some(false),
            format!("valid_fixup={:?}", e.valid_fixup),
        ),
        Err(err) => rep.check("torn sector detected", true, format!("error: {err}")),
    }

    std::process::exit(i32::from(!rep.0));
}
