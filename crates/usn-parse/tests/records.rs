//! Behavior of `usn_parse::records` on synthetic `$J` streams.

mod support;

use std::error::Error;
use std::io::Cursor;

use support::{Fields, other_version, u16s, v2, v3};
use usn_parse::{DiagCode, Diagnostic, Record, UsnEvent, records};

type TestResult = Result<(), Box<dyn Error>>;

fn parse(j: Vec<u8>) -> Result<Vec<Record>, usn_parse::Error> {
    records(Cursor::new(j)).collect()
}

#[test]
fn empty_input_yields_nothing() -> TestResult {
    assert!(parse(Vec::new())?.is_empty());
    Ok(())
}

#[test]
fn v2_record_yields_all_fields() -> TestResult {
    let f = Fields::default();
    let items = parse(v2(&f))?;
    let [Record::Event(e)] = items.as_slice() else {
        return Err(format!("expected one event, got {items:?}").into());
    };
    assert_eq!(e.offset, 0);
    assert_eq!(e.usn, 4096);
    assert_eq!(e.file.raw(), f.file);
    assert_eq!(e.parent.raw(), f.parent);
    assert_eq!(e.time.raw(), f.time);
    assert_eq!(e.reason, f.reason);
    assert_eq!(e.attributes, f.attributes);
    assert_eq!(e.name.units(), f.name.as_slice());
    Ok(())
}

fn only_event(items: &[Record]) -> Result<&UsnEvent, Box<dyn Error>> {
    match items {
        [Record::Event(e)] => Ok(e),
        _ => Err(format!("expected one event, got {items:?}").into()),
    }
}

#[test]
fn v3_record_takes_the_file_refs_from_the_low_64_bits() -> TestResult {
    let f = Fields {
        name: u16s("v3.log"),
        ..Fields::default()
    };
    let items = parse(v3(&f, 0, 0))?;
    let e = only_event(&items)?;
    assert_eq!(e.usn, 4096);
    assert_eq!(e.file.raw(), f.file);
    assert_eq!(e.parent.raw(), f.parent);
    assert_eq!(e.time.raw(), f.time);
    assert_eq!(e.reason, f.reason);
    assert_eq!(e.attributes, f.attributes);
    assert_eq!(e.name.units(), f.name.as_slice());
    Ok(())
}

#[test]
fn v3_with_nonzero_high_id_bits_is_malformed() -> TestResult {
    let (d, _) = diagnostic_then_event(v3(&Fields::default(), 1, 0))?;
    assert_eq!(
        d,
        Diagnostic {
            code: DiagCode::Malformed,
            offset: 0
        }
    );
    Ok(())
}

#[test]
fn consecutive_records_come_out_in_order() -> TestResult {
    let names = ["one", "second.txt", "3"];
    let mut j = Vec::new();
    let mut offsets = Vec::new();
    for (i, name) in names.iter().enumerate() {
        offsets.push(u64::try_from(j.len())?);
        let f = Fields {
            usn: i64::try_from(j.len())?,
            name: u16s(name),
            ..Fields::default()
        };
        j.extend(if i == 1 { v3(&f, 0, 0) } else { v2(&f) });
    }
    let got: Vec<(u64, u64, String)> = parse(j)?
        .iter()
        .map(|r| match r {
            Record::Event(e) => Ok((e.offset, e.usn, e.name.to_string())),
            other => Err(format!("unexpected {other:?}")),
        })
        .collect::<Result<_, _>>()?;
    let want: Vec<(u64, u64, String)> = offsets
        .iter()
        .zip(names)
        .map(|(&o, n)| (o, o, n.to_string()))
        .collect();
    assert_eq!(got, want);
    Ok(())
}

#[test]
fn leading_zero_region_is_skipped_without_diagnostics() -> TestResult {
    let zeros = 3 * 65536 + 8; // spans several reads
    let mut j = vec![0u8; zeros];
    j.extend(v2(&Fields::default()));
    let items = parse(j)?;
    assert_eq!(only_event(&items)?.offset, u64::try_from(zeros)?);
    Ok(())
}

#[test]
fn zero_padding_between_records_is_skipped() -> TestResult {
    let f = Fields::default();
    let mut j = v2(&f);
    j.resize(4096, 0); // the rest of the journal page
    j.extend(v2(&f));
    let offsets: Vec<u64> = parse(j)?
        .iter()
        .map(|r| match r {
            Record::Event(e) => Ok(e.offset),
            other => Err(format!("unexpected {other:?}")),
        })
        .collect::<Result<_, _>>()?;
    assert_eq!(offsets, [0, 4096]);
    Ok(())
}

/// `head` then one V2 record: the diagnostic for `head`, then the event right after it.
fn diagnostic_then_event(head: Vec<u8>) -> Result<(Diagnostic, u64), Box<dyn Error>> {
    let next = u64::try_from(head.len())?;
    let mut j = head;
    j.extend(v2(&Fields::default()));
    let items = parse(j)?;
    let [Record::Diagnostic(d), Record::Event(e)] = items.as_slice() else {
        return Err(format!("expected a diagnostic then an event, got {items:?}").into());
    };
    assert_eq!(e.offset, next);
    Ok((*d, e.offset))
}

#[test]
fn v4_record_is_unsupported_and_skipped() -> TestResult {
    let (d, _) = diagnostic_then_event(other_version(4, 0x50))?;
    assert_eq!(
        d,
        Diagnostic {
            code: DiagCode::UnsupportedVersion,
            offset: 0
        }
    );
    Ok(())
}

#[test]
fn unknown_major_version_is_unsupported() -> TestResult {
    for major in [0u16, 1, 5, 0xFFFF] {
        let (d, _) = diagnostic_then_event(other_version(major, 0x40))?;
        assert_eq!(
            d,
            Diagnostic {
                code: DiagCode::UnsupportedVersion,
                offset: 0
            },
            "major {major}"
        );
    }
    Ok(())
}

#[test]
fn record_length_below_header_is_malformed_and_resyncs() -> TestResult {
    // Claims 56 bytes (a V2 header is 60) but the next real record starts 16 bytes in.
    let mut head = other_version(2, 16);
    head[0..4].copy_from_slice(&56u32.to_le_bytes());
    let (d, at) = diagnostic_then_event(head)?;
    assert_eq!(
        (d, at),
        (
            Diagnostic {
                code: DiagCode::Malformed,
                offset: 0
            },
            16
        )
    );
    Ok(())
}

#[test]
fn record_length_over_the_cap_or_unaligned_is_malformed() -> TestResult {
    for len in [4096 + 8, 0x7FFF_FFF8, 0x3C + 1, 0x4C + 2] {
        let mut head = other_version(2, 16);
        head[0..4].copy_from_slice(&u32::try_from(len)?.to_le_bytes());
        let (d, at) = diagnostic_then_event(head)?;
        assert_eq!((d, at), (Diagnostic { code: DiagCode::Malformed, offset: 0 }, 16), "len {len}");
    }
    Ok(())
}

fn malformed_v2(edit: impl Fn(&mut Vec<u8>)) -> TestResult {
    let mut head = v2(&Fields::default());
    edit(&mut head);
    let (d, _) = diagnostic_then_event(head)?;
    assert_eq!(d, Diagnostic { code: DiagCode::Malformed, offset: 0 });
    Ok(())
}

#[test]
fn name_outside_the_record_is_malformed() -> TestResult {
    // Name offset past the record, then a name length running past it.
    malformed_v2(|r| r[0x3A..0x3C].copy_from_slice(&0x0100u16.to_le_bytes()))?;
    malformed_v2(|r| r[0x38..0x3A].copy_from_slice(&0x0040u16.to_le_bytes()))
}
