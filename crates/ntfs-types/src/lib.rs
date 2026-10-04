//! Value types shared across the crates. No I/O, no detection logic.
//!
//! - [`FileRef`]: MFT entry number plus sequence number.
//!
//! Planned (docs/adr/0002-foundational-decisions.md):
//! - `NtfsName`: a file name from evidence, kept as raw UTF-16 so unpaired surrogates survive.
//! - `NormPath`: a comparison key, normalized and upper-cased with the `$UpCase` table.
//! - `Filetime`: UTC time in 100 ns units (`u64`).

mod file_ref;

pub use file_ref::FileRef;
