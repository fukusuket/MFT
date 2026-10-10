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
    /// Bytes read but not yet consumed start at `buf[pos]`, which is stream offset `base + pos`.
    buf: Vec<u8>,
    pos: usize,
    base: u64,
    /// After a malformed record: scanning 8 bytes at a time for the next V2/V3 record.
    resyncing: bool,
}

/// Starts reading a `$J`.
pub fn records<R: Read>(reader: R) -> Records<R> {
    Records {
        reader,
        buf: Vec::new(),
        pos: 0,
        base: 0,
        resyncing: false,
    }
}

const READ_CHUNK: usize = 64 * 1024;
/// Longest record accepted: a V3 header plus a 255-unit name is under 600 bytes.
const MAX_RECORD: usize = 4096;

impl<R: Read> Records<R> {
    /// Makes at least `need` unconsumed bytes available, unless the input ends first.
    fn fill(&mut self, need: usize) -> std::io::Result<&[u8]> {
        if self.buf.len() - self.pos < need {
            self.buf.drain(..self.pos);
            self.base += widen(self.pos);
            self.pos = 0;
            while self.buf.len() < need {
                let old = self.buf.len();
                self.buf.resize(old + need.max(READ_CHUNK), 0);
                match self.reader.read(&mut self.buf[old..]) {
                    Ok(n) => {
                        self.buf.truncate(old + n);
                        if n == 0 {
                            break;
                        }
                    }
                    Err(e) => {
                        self.buf.truncate(old);
                        if e.kind() != std::io::ErrorKind::Interrupted {
                            return Err(e);
                        }
                    }
                }
            }
        }
        Ok(&self.buf[self.pos..])
    }
}

impl<R: Read> Iterator for Records<R> {
    type Item = Result<Record, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let offset = self.base + widen(self.pos);
            let head = match self.fill(8) {
                Ok(head) => head,
                Err(e) => return Some(Err(e.into())),
            };
            let len = usize::try_from(u32_at(head, 0)?).ok()?;
            if len == 0 {
                self.pos += 8; // sparse or padding: zeros up to the next 8-byte slot
                continue;
            }
            let layout = layout(u16_at(head, 4)?);
            // While resyncing, only a V2/V3 record ends the scan; anything else is skipped silently.
            let plausible = len % 8 == 0
                && len <= MAX_RECORD
                && layout.is_none_or(|l| len >= l.header);
            if !plausible || (self.resyncing && layout.is_none()) {
                self.pos += 8;
                if self.resyncing {
                    continue;
                }
                self.resyncing = true;
                return Some(Ok(diagnostic(DiagCode::Malformed, offset)));
            }
            let record = match self.fill(len) {
                Ok(record) => record.get(..len)?,
                Err(e) => return Some(Err(e.into())),
            };
            let item = match layout.map(|l| event(record, l, offset)) {
                Some(Some(event)) => Record::Event(event),
                _ if self.resyncing => {
                    self.pos += 8;
                    continue;
                }
                Some(None) => diagnostic(DiagCode::Malformed, offset),
                None => diagnostic(DiagCode::UnsupportedVersion, offset),
            };
            self.resyncing = false;
            self.pos += len;
            return Some(Ok(item));
        }
    }
}

fn diagnostic(code: DiagCode, offset: u64) -> Record {
    Record::Diagnostic(Diagnostic { code, offset })
}

/// Field offsets of one record version.
struct Layout {
    /// Bytes before the name.
    header: usize,
    /// Bytes of the file and parent ids; ids wider than 8 bytes must have zero upper bytes.
    id_size: usize,
    file: usize,
    parent: usize,
    usn: usize,
    time: usize,
    reason: usize,
    attributes: usize,
    name_len: usize,
}

/// `USN_RECORD_V2`: 64-bit file references.
const V2: Layout = Layout {
    header: 0x3C,
    id_size: 8,
    file: 0x08,
    parent: 0x10,
    usn: 0x18,
    time: 0x20,
    reason: 0x28,
    attributes: 0x34,
    name_len: 0x38,
};

/// `USN_RECORD_V3`: 128-bit file ids; NTFS keeps the file reference in the low 64 bits.
const V3: Layout = Layout {
    header: 0x4C,
    id_size: 16,
    file: 0x08,
    parent: 0x18,
    usn: 0x28,
    time: 0x30,
    reason: 0x38,
    attributes: 0x44,
    name_len: 0x48,
};

/// The layout of a parsed version; `None` for V4 and unknown versions.
fn layout(major: u16) -> Option<&'static Layout> {
    match major {
        2 => Some(&V2),
        3 => Some(&V3),
        _ => None,
    }
}

fn event(r: &[u8], l: &Layout, offset: u64) -> Option<UsnEvent> {
    for id in [l.file, l.parent] {
        if r.get(id + 8..id + l.id_size)?.iter().any(|&b| b != 0) {
            return None;
        }
    }
    let name_len = usize::from(u16_at(r, l.name_len)?);
    let name_off = usize::from(u16_at(r, l.name_len + 2)?);
    let name_bytes = r.get(name_off..name_off.checked_add(name_len)?)?;
    let (pairs, []) = name_bytes.as_chunks::<2>() else {
        return None; // odd byte count
    };
    let units: Vec<u16> = pairs.iter().map(|&c| u16::from_le_bytes(c)).collect();
    Some(UsnEvent {
        offset,
        usn: u64::try_from(i64_at(r, l.usn)?).ok()?,
        file: FileRef::from_raw(u64_at(r, l.file)?),
        parent: FileRef::from_raw(u64_at(r, l.parent)?),
        time: Filetime::from_raw(u64_at(r, l.time)?),
        reason: u32_at(r, l.reason)?,
        attributes: u32_at(r, l.attributes)?,
        name: NtfsName::from_units(&units),
    })
}

/// `usize` is at most 64 bits on every supported target.
fn widen(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        b.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        b.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn u64_at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        b.get(at..at.checked_add(8)?)?.try_into().ok()?,
    ))
}

fn i64_at(b: &[u8], at: usize) -> Option<i64> {
    Some(i64::from_le_bytes(
        b.get(at..at.checked_add(8)?)?.try_into().ok()?,
    ))
}
