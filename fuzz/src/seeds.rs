//! Writes seed inputs built with the test builder (ADR 0013: no committed corpus).
//! Usage: `cargo run --bin seeds -- corpus/mft_records`

#[path = "../../crates/mft-parse/tests/support/mod.rs"]
mod support;

use support::{file_name_with, record, resident, standard_information, u16s};

fn main() -> std::io::Result<()> {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "corpus/mft_records".into());
    std::fs::create_dir_all(&dir)?;
    for size in [1024, 4096] {
        let attrs = [
            standard_information(),
            file_name_with(5 | (5 << 48), &u16s("file.txt"), 1, 133_000_000_000_000_000),
            resident(0x80, &u16s("Zone.Identifier"), b"ZoneId=3", 2),
        ];
        let mut mft = record(size, true, 0, &attrs);
        mft.extend(record(size, false, 1, &attrs));
        std::fs::write(format!("{dir}/seed-{size}"), mft)?;
    }
    Ok(())
}
