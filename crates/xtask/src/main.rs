//! Developer tasks:
//! - `cargo run -p xtask --release -- synth-mft <records> <out>`: writes a synthetic `$MFT` built
//!   with the `mft-parse` test builder (Phase 1 gate: 1,048,576 records = 1 GiB).
//! - `cargo run -p xtask -- lolrmm <rmm_tools.json> <out-dir>`: writes one Sigma rule per LOLRMM
//!   tool (ADR 0017).

mod lolrmm;
#[path = "../../mft-parse/tests/support/mod.rs"]
mod support;

use std::fs::File;
use std::io::{BufWriter, Write};
use std::process::ExitCode;

const USAGE: &str =
    "usage: xtask synth-mft <records> <out> | xtask lolrmm <rmm_tools.json> <out-dir>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [task, records, out] if task == "synth-mft" => match records.parse::<u32>() {
            Ok(records) => match synth(records, out) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => fail(1, &format!("writing {out}: {e}")),
            },
            Err(_) => fail(2, USAGE),
        },
        [task, json, out] if task == "lolrmm" => match lolrmm::run(json, out) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => fail(1, &e),
        },
        _ => fail(2, USAGE),
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
