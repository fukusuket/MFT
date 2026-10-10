//! Behavior of `usn_parse::records` on synthetic `$J` streams.

mod support;

use std::error::Error;
use std::io::Cursor;

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
