//! Raw `$UsnJrnl:$J` → [`UsnEvent`] per USN record (ADR 0021). Corrupt records become
//! [`Diagnostic`]s, never a panic.
//!
//! Layout per Microsoft's `USN_RECORD_V2` and `USN_RECORD_V3`; `USN_RECORD_V4` is reported only.

use std::io::Read;

use ntfs_types::{FileRef, Filetime, NtfsName};

/// Fatal problems: the input cannot be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("reading $J: {0}")]
    Io(#[from] std::io::Error),
}

/// One item of the journal: an event, or a problem at a record offset.
#[derive(Debug)]
pub enum Record {
    Event(UsnEvent),
    Diagnostic(Diagnostic),
}

/// One USN record (V2 or V3).
#[derive(Debug)]
pub struct UsnEvent {
    /// Byte offset of the record in the `$J` stream.
    pub offset: u64,
    pub usn: u64,
    pub file: FileRef,
    pub parent: FileRef,
    pub time: Filetime,
    /// `USN_REASON_*` flags.
    pub reason: u32,
    /// `FILE_ATTRIBUTE_*` flags.
    pub attributes: u32,
    pub name: NtfsName,
}

/// A per-record problem (ADR 0002 #8): a code and the record's byte offset in the `$J`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: DiagCode,
    pub offset: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagCode {
    /// A V4 record or an unknown major version; skipped.
    UnsupportedVersion,
    /// The record's lengths, offsets or values are impossible; parsing resumes 8 bytes later.
    Malformed,
    /// The `$J` ends inside this record.
    Truncated,
}

/// Streams the records of a `$J`, one at a time.
#[derive(Debug)]
pub struct Records<R> {
    reader: R,
}

/// Starts reading a `$J`.
pub fn records<R: Read>(reader: R) -> Records<R> {
    Records { reader }
}

impl<R: Read> Iterator for Records<R> {
    type Item = Result<Record, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut probe = [0u8; 8];
        match self.reader.read(&mut probe) {
            Ok(0) => None,
            Ok(_) => None,
            Err(e) => Some(Err(e.into())),
        }
    }
}
