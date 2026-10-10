//! Behavior of `usn_parse::records` on synthetic `$J` streams.

mod support;

use std::error::Error;
use std::io::Cursor;

use support::{Fields, u16s, v2, v3};
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
    let f = Fields::default();
    let bad = v3(&f, 1, 0);
    let next = u64::try_from(bad.len())?;
    let mut j = bad;
    j.extend(v2(&f));
    let items = parse(j)?;
    let [Record::Diagnostic(d), Record::Event(e)] = items.as_slice() else {
        return Err(format!("expected a diagnostic then an event, got {items:?}").into());
    };
    assert_eq!(*d, Diagnostic { code: DiagCode::Malformed, offset: 0 });
    assert_eq!(e.offset, next);
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
