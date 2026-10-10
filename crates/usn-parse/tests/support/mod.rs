//! Builder for synthetic USN records, per Microsoft's `USN_RECORD_V2/V3/V4` layouts.

#![allow(dead_code)] // each test binary uses a different subset

/// Field values of one synthetic record.
#[derive(Debug, Clone)]
pub(crate) struct Fields {
    pub(crate) file: u64,
    pub(crate) parent: u64,
    pub(crate) usn: i64,
    pub(crate) time: u64,
    pub(crate) reason: u32,
    pub(crate) attributes: u32,
    pub(crate) name: Vec<u16>,
}

impl Default for Fields {
    fn default() -> Self {
        Self {
            file: 0x0002_0000_0000_1234,
            parent: 0x0005_0000_0000_0005,
            usn: 4096,
            time: 133_000_000_000_000_000,
            reason: 0x0000_0100, // FILE_CREATE
            attributes: 0x20,    // ARCHIVE
            name: u16s("a.txt"),
        }
    }
}

pub(crate) fn u16s(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn align8(n: usize) -> usize {
    n.div_ceil(8) * 8
}

fn put(b: &mut [u8], at: usize, v: &[u8]) {
    b[at..at + v.len()].copy_from_slice(v);
}

/// `USN_RECORD_V2`: 60-byte header, then the name; length padded to 8 bytes.
pub(crate) fn v2(f: &Fields) -> Vec<u8> {
    const HEADER: usize = 0x3C;
    let name: Vec<u8> = f.name.iter().flat_map(|u| u.to_le_bytes()).collect();
    let len = align8(HEADER + name.len());
    let mut r = vec![0u8; len];
    put(&mut r, 0x00, &u32::try_from(len).unwrap_or(0).to_le_bytes());
    put(&mut r, 0x04, &2u16.to_le_bytes());
    put(&mut r, 0x08, &f.file.to_le_bytes());
    put(&mut r, 0x10, &f.parent.to_le_bytes());
    put(&mut r, 0x18, &f.usn.to_le_bytes());
    put(&mut r, 0x20, &f.time.to_le_bytes());
    put(&mut r, 0x28, &f.reason.to_le_bytes());
    put(&mut r, 0x34, &f.attributes.to_le_bytes());
    put(&mut r, 0x38, &u16::try_from(name.len()).unwrap_or(0).to_le_bytes());
    put(&mut r, 0x3A, &u16::try_from(HEADER).unwrap_or(0).to_le_bytes());
    put(&mut r, HEADER, &name);
    r
}
