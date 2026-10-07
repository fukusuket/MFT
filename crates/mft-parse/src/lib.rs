//! Raw `$MFT` → [`Entry`] per FILE record. Corrupt records become [`Diagnostic`]s, never a panic.
//!
//! Names come only from `Utf16LeStr::as_utf16le_bytes()` (ADR 0004).

use std::io::Read;

use mft::MftEntry;
use mft::attribute::header::ResidentialHeader;
use mft::attribute::x30::{FileNameAttr, FileNamespace};
use mft::attribute::{MftAttribute, MftAttributeContent};
use ntfs_types::{FileRef, Filetime, NtfsName};

/// Fatal problems: the input cannot be read as a `$MFT` at all.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("reading $MFT: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported $MFT record size {0} (expected 1024 or 4096)")]
    UnsupportedRecordSize(u32),
}

/// One FILE record.
#[derive(Debug)]
pub struct Entry {
    pub file_ref: FileRef,
    pub in_use: bool,
    pub is_dir: bool,
    pub si_created: Option<Filetime>,
    /// Every `$FILE_NAME`, in attribute order.
    pub names: Vec<FileName>,
    /// Problems found in this record; the fields above hold what could still be read.
    pub diagnostics: Vec<Diagnostic>,
}

/// A per-record problem (ADR 0002 #8): a code and the record's byte offset in the `$MFT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: DiagCode,
    pub offset: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagCode {
    /// Signature is neither `FILE` nor unused (e.g. `BAAD`).
    BadSignature,
    /// A sector's update sequence number does not match (torn write); the data may be stale.
    FixupMismatch,
    /// The record header or an attribute could not be parsed; that attribute is skipped.
    Malformed,
    /// The `$MFT` ends inside this record.
    Truncated,
}

/// One `$FILE_NAME` attribute.
#[derive(Debug)]
pub struct FileName {
    pub name: NtfsName,
    pub parent: FileRef,
    pub namespace: Namespace,
    pub created: Filetime,
}

/// `$FILE_NAME` namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Namespace {
    Posix,
    Win32,
    Dos,
    Win32AndDos,
}

/// Streams the records of a `$MFT`, one at a time.
#[derive(Debug)]
pub struct Records<R> {
    reader: R,
    record_size: u16,
    /// Record 0, read ahead to learn the record size.
    first: Option<Vec<u8>>,
    next_entry: u64,
    done: bool,
}

const MIN_RECORD_SIZE: u16 = 1024;

/// Starts reading a `$MFT`. The record size (1024 or 4096 bytes) comes from record 0.
pub fn records<R: Read>(mut reader: R) -> Result<Records<R>, Error> {
    let mut first = vec![0u8; usize::from(MIN_RECORD_SIZE)];
    let mut filled = read_full(&mut reader, &mut first)?;
    let mut record_size = MIN_RECORD_SIZE;
    if filled == first.len() {
        let allocated = u32::from_le_bytes([first[0x1C], first[0x1D], first[0x1E], first[0x1F]]);
        record_size = match allocated {
            1024 => 1024,
            4096 => 4096,
            other => return Err(Error::UnsupportedRecordSize(other)),
        };
        first.resize(usize::from(record_size), 0);
        let rest = read_full(&mut reader, &mut first[usize::from(MIN_RECORD_SIZE)..])?;
        filled += rest;
    }
    first.truncate(filled);
    Ok(Records {
        reader,
        record_size,
        first: Some(first),
        next_entry: 0,
        done: false,
    })
}

impl<R: Read> Iterator for Records<R> {
    type Item = Result<Entry, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        while !self.done {
            let entry = self.next_entry;
            self.next_entry += 1;
            let offset = entry.saturating_mul(u64::from(self.record_size));
            let buf = match self.first.take() {
                Some(first) => first,
                None => {
                    let mut buf = vec![0u8; usize::from(self.record_size)];
                    match read_full(&mut self.reader, &mut buf) {
                        Ok(filled) => buf.truncate(filled),
                        Err(e) => {
                            self.done = true;
                            return Some(Err(e.into()));
                        }
                    }
                    buf
                }
            };
            if buf.len() < usize::from(self.record_size) {
                self.done = true;
                if buf.is_empty() {
                    break;
                }
                return Some(Ok(unreadable(entry, offset, DiagCode::Truncated)));
            }
            if buf.starts_with(&[0; 4]) {
                continue; // never-used slot
            }
            return Some(Ok(parse_record(entry, offset, buf)));
        }
        None
    }
}

/// Reads until `buf` is full or the input ends; returns the number of bytes read.
fn read_full(reader: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

/// An entry for a record whose contents can't be used: only its position and the reason.
fn unreadable(entry: u64, offset: u64, code: DiagCode) -> Entry {
    Entry {
        file_ref: file_ref(entry, 0),
        in_use: false,
        is_dir: false,
        si_created: None,
        names: Vec::new(),
        diagnostics: vec![Diagnostic { code, offset }],
    }
}

fn parse_record(entry: u64, offset: u64, buf: Vec<u8>) -> Entry {
    if !buf.starts_with(b"FILE") {
        return unreadable(entry, offset, DiagCode::BadSignature);
    }
    let mut parsed = Entry {
        file_ref: file_ref(entry, u16::from_le_bytes([buf[0x10], buf[0x11]])),
        in_use: buf[0x16] & 0x01 != 0,
        is_dir: buf[0x16] & 0x02 != 0,
        si_created: None,
        names: Vec::new(),
        diagnostics: Vec::new(),
    };
    let diagnostic = |code| Diagnostic { code, offset };
    match MftEntry::from_buffer(buf, entry) {
        Ok(record) => read_attributes(&record, &mut parsed, diagnostic),
        Err(_) => parsed.diagnostics.push(diagnostic(DiagCode::Malformed)),
    }
    parsed
}

/// Fills times and names from `record`; each attribute that fails to parse adds `Malformed`.
fn read_attributes(
    record: &MftEntry,
    parsed: &mut Entry,
    diagnostic: impl Fn(DiagCode) -> Diagnostic,
) {
    if record.valid_fixup == Some(false) {
        parsed.diagnostics.push(diagnostic(DiagCode::FixupMismatch));
    }
    for attr in record.iter_attributes() {
        let Ok(attr) = attr else {
            parsed.diagnostics.push(diagnostic(DiagCode::Malformed));
            continue; // `mft` moves past a bad attribute value; it ends the walk on a bad header
        };
        match &attr.data {
            MftAttributeContent::AttrX10(_) => {
                parsed.si_created = resident_u64(record, &attr, 0).map(Filetime::from_raw);
            }
            MftAttributeContent::AttrX30(f) => {
                parsed.names.extend(file_name(record, &attr, f));
            }
            _ => {}
        }
    }
}

fn file_ref(entry: u64, sequence: u16) -> FileRef {
    FileRef::from_raw(entry | (u64::from(sequence) << 48))
}

fn file_name(entry: &MftEntry, attr: &MftAttribute<'_>, f: &FileNameAttr<'_>) -> Option<FileName> {
    let created = resident_u64(entry, attr, 8)?;
    let units: Vec<u16> = f
        .name
        .as_utf16le_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&c| u16::from_le_bytes(c))
        .collect();
    Some(FileName {
        name: NtfsName::from_units(&units),
        parent: file_ref(f.parent.entry, f.parent.sequence),
        namespace: match f.namespace {
            FileNamespace::POSIX => Namespace::Posix,
            FileNamespace::Win32 => Namespace::Win32,
            FileNamespace::DOS => Namespace::Dos,
            FileNamespace::Win32AndDos => Namespace::Win32AndDos,
        },
        created: Filetime::from_raw(created),
    })
}

/// Reads a `u64` at `at` within a resident attribute's value, straight from the record.
/// The `mft` crate converts times to microseconds, so FILETIMEs are read here at full precision.
fn resident_u64(entry: &MftEntry, attr: &MftAttribute<'_>, at: usize) -> Option<u64> {
    let ResidentialHeader::Resident(r) = &attr.header.residential_header else {
        return None;
    };
    let start = usize::try_from(attr.header.start_offset)
        .ok()?
        .checked_add(usize::from(r.data_offset))?
        .checked_add(at)?;
    let bytes = entry.data.get(start..start.checked_add(8)?)?;
    Some(u64::from_le_bytes(bytes.try_into().ok()?))
}
