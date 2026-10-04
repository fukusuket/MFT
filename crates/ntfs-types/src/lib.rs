//! Value types shared across the crates. No I/O, no detection logic.
//!
//! - [`FileRef`]: MFT entry number plus sequence number.
//! - [`Filetime`]: UTC time in 100 ns units since 1601 (`u64`).
//!
//! Planned (docs/adr/0002-foundational-decisions.md):
//! - `NtfsName`: a file name from evidence, kept as raw UTF-16 so unpaired surrogates survive.
//! - `NormPath`: a comparison key, normalized and upper-cased with the `$UpCase` table.

mod file_ref;
mod filetime;

pub use file_ref::FileRef;
pub use filetime::Filetime;
