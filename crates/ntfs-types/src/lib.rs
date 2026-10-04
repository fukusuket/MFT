//! Value types shared across the crates. No I/O, no detection logic.
//!
//! - [`FileRef`]: MFT entry number plus sequence number.
//! - [`Filetime`]: UTC time in 100 ns units since 1601 (`u64`).
//! - [`NtfsName`]: a file name from evidence, kept as raw UTF-16 so unpaired surrogates survive.
//! - [`NormPath`]: a comparison key, upper-cased with the Windows default `$UpCase` table.
//!
//! Design decisions: docs/adr/0002-foundational-decisions.md, docs/adr/0009-default-upcase-table.md.

mod file_ref;
mod filetime;
mod norm_path;
mod ntfs_name;
mod upcase;

pub use file_ref::FileRef;
pub use filetime::Filetime;
pub use norm_path::NormPath;
pub use ntfs_name::NtfsName;
