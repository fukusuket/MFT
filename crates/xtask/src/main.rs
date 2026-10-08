//! `cargo run -p xtask --release -- synth-mft <records> <out>`: writes a synthetic `$MFT` built
//! with the `mft-parse` test builder (Phase 1 gate: 1,048,576 records = 1 GiB).

#[path = "../../mft-parse/tests/support/mod.rs"]
mod support;

use std::fs::File;
use std::io::{BufWriter, Write};
use std::process::ExitCode;

const USAGE: &str = "usage: xtask synth-mft <records> <out>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (records, out) = match args.as_slice() {
        [task, records, out] if task == "synth-mft" => match records.parse::<u32>() {
            Ok(records) => (records, out),
            Err(_) => return fail(2, USAGE),
        },
        _ => return fail(2, USAGE),
    };
    match synth(records, out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(1, &format!("writing {out}: {e}")),
    }
}

fn synth(records: u32, out: &str) -> std::io::Result<()> {
    let mut writer = BufWriter::new(File::create(out)?);
    support::synth::synthetic_mft(records, &mut writer)?;
    writer.flush()
}

fn fail(code: u8, message: &str) -> ExitCode {
    let _ = writeln!(std::io::stderr(), "{message}");
    ExitCode::from(code)
}
