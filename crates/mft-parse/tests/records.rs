//! Behavior of `mft_parse::records` on synthetic `$MFT` streams.

mod support;

use std::error::Error;
use std::io::Cursor;

use mft_parse::{DiagCode, Diagnostic, Entry, Namespace, records};
use ntfs_types::Filetime;
use support::{
    extension, file_name_with, record, record_with_flags, resident, standard_information,
    standard_information_created, u16s,
};

type TestResult = Result<(), Box<dyn Error>>;

fn parse(mft: Vec<u8>) -> Result<Vec<Entry>, Box<dyn Error>> {
    Ok(records(Cursor::new(mft))?.collect::<Result<Vec<_>, _>>()?)
}

#[test]
fn yields_entry_number_and_sequence_per_record() -> TestResult {
    let mut mft = record(1024, true, 0, &[standard_information()]);
    mft.extend(record(1024, true, 1, &[standard_information()]));

    let entries = parse(mft)?;

    let refs: Vec<(u64, u16)> = entries
        .iter()
        .map(|e| (e.file_ref.entry(), e.file_ref.sequence()))
        .collect();
    assert_eq!(refs, [(0, 1), (1, 1)]);
    Ok(())
}

#[test]
fn in_use_comes_from_the_header_and_deleted_records_are_kept() -> TestResult {
    let mut mft = record(1024, true, 0, &[standard_information()]);
    mft.extend(record(1024, false, 1, &[standard_information()]));

    let in_use: Vec<bool> = parse(mft)?.iter().map(|e| e.in_use).collect();

    assert_eq!(in_use, [true, false]);
    Ok(())
}

#[test]
fn si_created_comes_from_standard_information() -> TestResult {
    let mut mft = record(
        1024,
        true,
        0,
        &[standard_information_created(133_444_555_666_777_888)],
    );
    mft.extend(record(1024, true, 1, &[]));

    let created: Vec<Option<Filetime>> = parse(mft)?.iter().map(|e| e.si_created).collect();

    assert_eq!(
        created,
        [Some(Filetime::from_raw(133_444_555_666_777_888)), None]
    );
    Ok(())
}

#[test]
fn every_file_name_is_kept_with_parent_namespace_and_created() -> TestResult {
    let parent = 5 | (5 << 48);
    let mft = record(
        1024,
        true,
        0,
        &[
            standard_information(),
            file_name_with(parent, &u16s("Long Name.txt"), 1, 133_000_000_000_000_001),
            file_name_with(parent, &u16s("LONGNA~1.TXT"), 2, 133_000_000_000_000_002),
        ],
    );

    let entries = parse(mft)?;

    let names: Vec<(Vec<u16>, u64, u16, Namespace, u64)> = entries[0]
        .names
        .iter()
        .map(|n| {
            (
                n.name.units().to_vec(),
                n.parent.entry(),
                n.parent.sequence(),
                n.namespace,
                n.created.raw(),
            )
        })
        .collect();
    assert_eq!(
        names,
        [
            (
                u16s("Long Name.txt"),
                5,
                5,
                Namespace::Win32,
                133_000_000_000_000_001
            ),
            (
                u16s("LONGNA~1.TXT"),
                5,
                5,
                Namespace::Dos,
                133_000_000_000_000_002
            ),
        ]
    );
    Ok(())
}

#[test]
fn unpaired_surrogate_in_a_name_survives() -> TestResult {
    let name = [0x0061, 0xD800, 0x0062];
    let mft = record(1024, true, 0, &[file_name_with(5, &name, 1, 0)]);

    let entries = parse(mft)?;

    assert_eq!(entries[0].names[0].name.units(), name);
    Ok(())
}

#[test]
fn record_size_comes_from_record_zero_so_4096_byte_records_parse() -> TestResult {
    let mut mft = record(4096, true, 0, &[standard_information()]);
    mft.extend(record(4096, false, 1, &[standard_information()]));

    let refs: Vec<(u64, bool)> = parse(mft)?
        .iter()
        .map(|e| (e.file_ref.entry(), e.in_use))
        .collect();

    assert_eq!(refs, [(0, true), (1, false)]);
    Ok(())
}

#[test]
fn record_size_other_than_1024_or_4096_is_an_error() {
    let mft = record(2048, true, 0, &[standard_information()]);

    let result = records(Cursor::new(mft));

    assert!(
        matches!(result, Err(mft_parse::Error::UnsupportedRecordSize(2048))),
        "{:?}",
        result.err()
    );
}

#[test]
fn empty_input_yields_no_records() -> TestResult {
    assert!(parse(Vec::new())?.is_empty());
    Ok(())
}

#[test]
fn never_used_all_zero_slots_are_skipped() -> TestResult {
    let mut mft = record(1024, true, 0, &[standard_information()]);
    mft.extend(vec![0u8; 1024]);
    mft.extend(record(1024, true, 2, &[standard_information()]));

    let entries: Vec<u64> = parse(mft)?.iter().map(|e| e.file_ref.entry()).collect();

    assert_eq!(entries, [0, 2]);
    Ok(())
}

#[test]
fn bad_signature_keeps_only_the_position_and_reports_it() -> TestResult {
    let mut mft = record(1024, true, 0, &[standard_information()]);
    let mut bad = record(
        1024,
        true,
        1,
        &[standard_information(), file_name_with(5, &u16s("x"), 1, 0)],
    );
    bad[0..4].copy_from_slice(b"BAAD");
    mft.extend(bad);

    let entries = parse(mft)?;

    let bad = &entries[1];
    assert_eq!((bad.file_ref.entry(), bad.file_ref.sequence()), (1, 0));
    assert!(!bad.in_use);
    assert_eq!(bad.si_created, None);
    assert!(bad.names.is_empty());
    assert_eq!(
        bad.diagnostics,
        [Diagnostic {
            code: DiagCode::BadSignature,
            offset: 1024
        }]
    );
    assert!(entries[0].diagnostics.is_empty());
    Ok(())
}

#[test]
fn fixup_mismatch_keeps_the_entry_and_reports_it() -> TestResult {
    let mut mft = record(1024, true, 0, &[file_name_with(5, &u16s("x"), 1, 0)]);
    mft[1022] ^= 0xFF; // last two bytes of sector 2 no longer carry the update sequence number

    let entries = parse(mft)?;

    assert_eq!(entries[0].names[0].name.units(), u16s("x"));
    assert_eq!(
        entries[0].diagnostics,
        [Diagnostic {
            code: DiagCode::FixupMismatch,
            offset: 0
        }]
    );
    Ok(())
}

#[test]
fn malformed_attribute_is_reported_and_earlier_attributes_are_kept() -> TestResult {
    let too_short_si = resident(0x10, &[], &[0u8; 8], 2);
    let mft = record(
        1024,
        true,
        0,
        &[file_name_with(5, &u16s("x"), 1, 0), too_short_si],
    );

    let entries = parse(mft)?;

    assert_eq!(entries[0].names[0].name.units(), u16s("x"));
    assert_eq!(
        entries[0].diagnostics,
        [Diagnostic {
            code: DiagCode::Malformed,
            offset: 0
        }]
    );
    Ok(())
}

#[test]
fn unparsable_header_is_reported_as_malformed() -> TestResult {
    let mut mft = record(1024, true, 0, &[standard_information()]);
    mft[0x06..0x08].copy_from_slice(&0xFFFFu16.to_le_bytes()); // update sequence array far past the record

    let entries = parse(mft)?;

    assert_eq!(
        entries[0].diagnostics,
        [Diagnostic {
            code: DiagCode::Malformed,
            offset: 0
        }]
    );
    Ok(())
}

#[test]
fn trailing_partial_record_is_reported_as_truncated() -> TestResult {
    let mut mft = record(1024, true, 0, &[standard_information()]);
    mft.extend(&record(1024, true, 1, &[standard_information()])[..100]);

    let entries = parse(mft)?;

    let diagnostics: Vec<(u64, Vec<Diagnostic>)> = entries
        .iter()
        .map(|e| (e.file_ref.entry(), e.diagnostics.clone()))
        .collect();
    assert_eq!(
        diagnostics,
        [
            (0, vec![]),
            (
                1,
                vec![Diagnostic {
                    code: DiagCode::Truncated,
                    offset: 1024
                }]
            )
        ]
    );
    Ok(())
}

/// Returns `data`, then fails every read.
struct FailAfter(Cursor<Vec<u8>>);

impl std::io::Read for FailAfter {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self.0.read(buf)? {
            0 => Err(std::io::Error::other("device error")),
            n => Ok(n),
        }
    }
}

#[test]
fn read_errors_are_errors_not_end_of_input() -> TestResult {
    let at_start = records(FailAfter(Cursor::new(Vec::new())));
    assert!(
        matches!(at_start, Err(mft_parse::Error::Io(_))),
        "{:?}",
        at_start.err()
    );

    let one = record(1024, true, 0, &[standard_information()]);
    let items: Vec<bool> = records(FailAfter(Cursor::new(one)))?
        .map(|item| item.is_ok())
        .collect();
    assert_eq!(items, [true, false]);
    Ok(())
}

#[test]
fn an_unparsable_attribute_does_not_hide_later_names() -> TestResult {
    let after_year_9999 = u64::MAX;
    let mft = record(
        1024,
        true,
        0,
        &[
            standard_information_created(after_year_9999),
            file_name_with(5, &u16s("x"), 1, 0),
        ],
    );

    let entries = parse(mft)?;

    assert_eq!(entries[0].names.len(), 1);
    assert_eq!(
        entries[0].diagnostics,
        [Diagnostic {
            code: DiagCode::Malformed,
            offset: 0
        }]
    );
    Ok(())
}

/// Fails the first read with `Interrupted`, then reads normally.
struct InterruptedOnce(bool, Cursor<Vec<u8>>);

impl std::io::Read for InterruptedOnce {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if std::mem::replace(&mut self.0, false) {
            return Err(std::io::ErrorKind::Interrupted.into());
        }
        self.1.read(buf)
    }
}

#[test]
fn interrupted_reads_are_retried() -> TestResult {
    let mft = record(1024, true, 0, &[standard_information()]);

    let entries: Vec<_> =
        records(InterruptedOnce(true, Cursor::new(mft)))?.collect::<Result<_, _>>()?;

    assert_eq!(entries.len(), 1);
    Ok(())
}

#[test]
fn is_dir_comes_from_the_header_flag() -> TestResult {
    let mut mft = record_with_flags(1024, 0x01, 0, &[standard_information()]);
    mft.extend(record_with_flags(1024, 0x03, 1, &[standard_information()]));

    let dirs: Vec<bool> = parse(mft)?.iter().map(|e| e.is_dir).collect();

    assert_eq!(dirs, [false, true]);
    Ok(())
}

#[test]
fn base_comes_from_the_header() -> TestResult {
    let mut mft = record(1024, true, 0, &[standard_information()]);
    mft.extend(extension(1024, true, 1, 40 | (3 << 48), &[]));

    let bases: Vec<Option<(u64, u16)>> = parse(mft)?
        .iter()
        .map(|e| e.base.map(|b| (b.entry(), b.sequence())))
        .collect();

    assert_eq!(bases, [None, Some((40, 3))]);
    Ok(())
}

#[test]
fn an_extension_of_the_mft_itself_has_base_entry_0() -> TestResult {
    let mut mft = record(1024, true, 0, &[standard_information()]);
    mft.extend(extension(1024, true, 1, 1 << 48, &[]));

    let bases: Vec<Option<(u64, u16)>> = parse(mft)?
        .iter()
        .map(|e| e.base.map(|b| (b.entry(), b.sequence())))
        .collect();

    assert_eq!(bases, [None, Some((0, 1))]);
    Ok(())
}

fn names(entry: &Entry) -> Vec<String> {
    entry.names.iter().map(|n| n.name.to_string()).collect()
}

#[test]
fn live_extension_names_and_diagnostics_move_to_the_base() -> TestResult {
    let mut mft = record(
        1024,
        true,
        0,
        &[
            standard_information(),
            file_name_with(5, &u16s("LONGNA~1.TXT"), 2, 0),
        ],
    );
    let mut ext = extension(
        1024,
        true,
        1,
        1 << 48,
        &[file_name_with(5, &u16s("Long name.txt"), 1, 0)],
    );
    ext[1022] ^= 0xFF; // fixup mismatch in the extension
    mft.extend(ext);
    mft.extend(record(
        1024,
        true,
        2,
        &[file_name_with(5, &u16s("other"), 1, 0)],
    ));

    let merged = mft_parse::merge_extensions(parse(mft)?, 1024);

    let rows: Vec<(u64, Vec<String>, Vec<Diagnostic>)> = merged
        .iter()
        .map(|e| (e.file_ref.entry(), names(e), e.diagnostics.clone()))
        .collect();
    assert_eq!(
        rows,
        [
            (
                0,
                vec!["LONGNA~1.TXT".to_string(), "Long name.txt".to_string()],
                vec![Diagnostic {
                    code: DiagCode::FixupMismatch,
                    offset: 1024
                }]
            ),
            (2, vec!["other".to_string()], vec![]),
        ]
    );
    Ok(())
}

#[test]
fn unmergeable_extensions_keep_their_row_as_orphans() -> TestResult {
    let named = |n: &str| file_name_with(5, &u16s(n), 1, 0);
    let mut mft = record(1024, true, 0, &[standard_information()]); // live base, sequence 1
    mft.extend(extension(
        1024,
        true,
        1,
        77 | (1 << 48),
        &[named("missing base")],
    ));
    mft.extend(extension(
        1024,
        true,
        2,
        2 << 48,
        &[named("wrong sequence")],
    ));
    mft.extend(extension(1024, false, 3, 1 << 48, &[named("stale")]));
    mft.extend(extension(
        1024,
        true,
        4,
        1 | (1 << 48),
        &[named("base is an extension")],
    ));

    let merged = mft_parse::merge_extensions(parse(mft)?, 1024);

    let orphan = |offset| {
        vec![Diagnostic {
            code: DiagCode::OrphanExtension,
            offset,
        }]
    };
    let rows: Vec<(u64, Vec<Diagnostic>)> = merged
        .iter()
        .map(|e| (e.file_ref.entry(), e.diagnostics.clone()))
        .collect();
    assert_eq!(
        rows,
        [
            (0, vec![]),
            (1, orphan(1024)),
            (2, orphan(2048)),
            (3, orphan(3072)),
            (4, orphan(4096))
        ]
    );
    assert_eq!(names(&merged[0]), Vec::<String>::new());
    Ok(())
}

#[test]
fn several_extensions_merge_in_entry_order_after_the_base_names() -> TestResult {
    let named = |n: &str| file_name_with(5, &u16s(n), 1, 0);
    let base = 2 | (1 << 48);
    let mut mft = record(1024, true, 0, &[standard_information()]);
    mft.extend(extension(1024, true, 1, base, &[named("from 1")]));
    mft.extend(record(1024, true, 2, &[named("base")]));
    mft.extend(extension(
        1024,
        true,
        3,
        base,
        &[named("from 3a"), named("from 3b")],
    ));

    let merged = mft_parse::merge_extensions(parse(mft)?, 1024);

    assert_eq!(
        merged
            .iter()
            .map(|e| e.file_ref.entry())
            .collect::<Vec<_>>(),
        [0, 2]
    );
    assert_eq!(names(&merged[1]), ["base", "from 1", "from 3a", "from 3b"]);
    Ok(())
}

#[test]
fn an_unreadable_record_is_never_a_base() -> TestResult {
    let mut mft = record(1024, true, 0, &[standard_information()]);
    let mut bad = record(1024, true, 1, &[standard_information()]);
    bad[0..4].copy_from_slice(b"BAAD"); // read as: entry 1, sequence 0, not in use
    mft.extend(bad);
    mft.extend(extension(
        1024,
        false,
        2,
        1,
        &[file_name_with(5, &u16s("x"), 1, 0)],
    ));

    let merged = mft_parse::merge_extensions(parse(mft)?, 1024);

    let rows: Vec<(u64, Vec<String>)> = merged
        .iter()
        .map(|e| (e.file_ref.entry(), names(e)))
        .collect();
    assert_eq!(rows, [(0, vec![]), (1, vec![]), (2, vec!["x".to_string()])]);
    assert_eq!(
        merged[2].diagnostics,
        [Diagnostic {
            code: DiagCode::OrphanExtension,
            offset: 2048
        }]
    );
    Ok(())
}

#[test]
fn a_deleted_files_extension_merges_into_its_deleted_base() -> TestResult {
    let mut mft = record(1024, true, 0, &[standard_information()]);
    mft.extend(record(
        1024,
        false,
        1,
        &[file_name_with(5, &u16s("DELETE~1.EXE"), 2, 0)],
    ));
    mft.extend(extension(
        1024,
        false,
        2,
        1 | (1 << 48),
        &[file_name_with(5, &u16s("deleted tool.exe"), 1, 0)],
    ));

    let merged = mft_parse::merge_extensions(parse(mft)?, 1024);

    assert_eq!(merged.len(), 2);
    assert_eq!(names(&merged[1]), ["DELETE~1.EXE", "deleted tool.exe"]);
    assert!(merged[1].diagnostics.is_empty());
    Ok(())
}
