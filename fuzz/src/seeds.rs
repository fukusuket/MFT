//! Writes seed inputs built with the test builders (ADR 0013: no committed corpus).
//! Usage: `cargo run --bin seeds -- corpus/<target>`; the directory name picks the target.

#[path = "../../crates/mft-parse/tests/support/mod.rs"]
mod support;

#[path = "../../crates/usn-parse/tests/support/mod.rs"]
mod usn_support;

use support::{file_name_with, record, resident, standard_information, u16s};

fn main() -> std::io::Result<()> {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "corpus/mft_records".into());
    std::fs::create_dir_all(&dir)?;
    if dir.ends_with("usn_records") {
        return usn_seeds(&dir);
    }
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

fn usn_seeds(dir: &str) -> std::io::Result<()> {
    use usn_support::{Fields, other_version, v2, v3};
    let f = Fields::default();
    let mut j = vec![0u8; 64];
    j.extend(v2(&f));
    j.extend(v3(&f, 0, 0));
    j.extend(other_version(4, 0x50));
    j.extend(v2(&f));
    std::fs::write(format!("{dir}/seed-journal"), j)
}
