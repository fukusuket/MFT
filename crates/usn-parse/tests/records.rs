//! Behavior of `usn_parse::records` on synthetic `$J` streams.

mod support;

use std::error::Error;
use std::io::Cursor;

use support::{Fields, v2};
use usn_parse::{Record, records};

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
