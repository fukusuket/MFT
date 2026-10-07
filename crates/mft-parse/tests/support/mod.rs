//! Builder for synthetic NTFS FILE records (moved from `spikes/s2-mft`).
//! Layout per Microsoft's FILE_RECORD_SEGMENT_HEADER and resident attribute header.

#![allow(dead_code)] // each test binary uses a different subset

const USA_OFFSET: usize = 0x30;
const SECTOR: usize = 512;
const USN: [u8; 2] = [0x01, 0x00];

fn align8(n: usize) -> usize {
    n.div_ceil(8) * 8
}

fn units_le(units: &[u16]) -> Vec<u8> {
    units.iter().flat_map(|u| u.to_le_bytes()).collect()
}

/// Resident attribute with an optional name.
pub(crate) fn resident(type_code: u32, name: &[u16], content: &[u8], id: u16) -> Vec<u8> {
    let name_bytes = units_le(name);
    let name_offset = 0x18;
    let content_offset = align8(name_offset + name_bytes.len());
    let length = align8(content_offset + content.len());
    let mut a = vec![0u8; length];
    a[0..4].copy_from_slice(&type_code.to_le_bytes());
    a[4..8].copy_from_slice(&u32::try_from(length).unwrap_or(0).to_le_bytes());
    a[8] = 0; // resident
    a[9] = u8::try_from(name.len()).unwrap_or(0);
    a[0x0A..0x0C].copy_from_slice(&u16::try_from(name_offset).unwrap_or(0).to_le_bytes());
    a[0x0E..0x10].copy_from_slice(&id.to_le_bytes());
    a[0x10..0x14].copy_from_slice(&u32::try_from(content.len()).unwrap_or(0).to_le_bytes());
    a[0x14..0x16].copy_from_slice(&u16::try_from(content_offset).unwrap_or(0).to_le_bytes());
    a[name_offset..name_offset + name_bytes.len()].copy_from_slice(&name_bytes);
    a[content_offset..content_offset + content.len()].copy_from_slice(content);
    a
}

/// `$STANDARD_INFORMATION` (NTFS 3.x, 72 bytes).
pub(crate) fn standard_information() -> Vec<u8> {
    standard_information_created(132_000_000_000_000_000)
}

/// `$STANDARD_INFORMATION` whose created time is `created`; the other three times are fixed.
pub(crate) fn standard_information_created(created: u64) -> Vec<u8> {
    let mut c = vec![0u8; 72];
    c[0..8].copy_from_slice(&created.to_le_bytes());
    for i in 1..4 {
        c[i * 8..i * 8 + 8].copy_from_slice(&132_000_000_000_000_000u64.to_le_bytes());
    }
    resident(0x10, &[], &c, 0)
}

/// `$FILE_NAME` with an arbitrary UTF-16 name (Win32 namespace).
pub(crate) fn file_name(parent: u64, name: &[u16]) -> Vec<u8> {
    file_name_with(parent, name, 1, 132_000_000_000_000_000)
}

/// `$FILE_NAME` with a given namespace (0 POSIX, 1 Win32, 2 DOS, 3 Win32+DOS) and created time.
pub(crate) fn file_name_with(parent: u64, name: &[u16], namespace: u8, created: u64) -> Vec<u8> {
    let mut c = vec![0u8; 66];
    c[0..8].copy_from_slice(&parent.to_le_bytes());
    c[8..16].copy_from_slice(&created.to_le_bytes());
    for i in 1..4 {
        let o = 8 + i * 8;
        c[o..o + 8].copy_from_slice(&132_000_000_000_000_000u64.to_le_bytes());
    }
    c[0x40] = u8::try_from(name.len()).unwrap_or(0);
    c[0x41] = namespace;
    c.extend(units_le(name));
    resident(0x30, &[], &c, 1)
}

/// Resident `$DATA`, unnamed (main stream) or named (ADS).
pub(crate) fn data(name: &[u16], content: &[u8], id: u16) -> Vec<u8> {
    resident(0x80, name, content, id)
}

/// A complete FILE record with fixups applied (`size` = 1024 or 4096).
pub(crate) fn record(size: usize, in_use: bool, entry: u32, attrs: &[Vec<u8>]) -> Vec<u8> {
    record_with_flags(size, u16::from(in_use), entry, attrs)
}

/// An extension record of `base` (entry | sequence << 48).
pub(crate) fn extension(
    size: usize,
    in_use: bool,
    entry: u32,
    base: u64,
    attrs: &[Vec<u8>],
) -> Vec<u8> {
    let mut r = record(size, in_use, entry, attrs);
    r[0x20..0x28].copy_from_slice(&base.to_le_bytes());
    r
}

/// Like [`record`] with explicit header flags (0x01 in use, 0x02 directory).
pub(crate) fn record_with_flags(size: usize, flags: u16, entry: u32, attrs: &[Vec<u8>]) -> Vec<u8> {
    let sectors = size / SECTOR;
    let usa_count = sectors + 1;
    let first_attr = align8(USA_OFFSET + usa_count * 2);
    let mut r = vec![0u8; size];
    r[0..4].copy_from_slice(b"FILE");
    r[4..6].copy_from_slice(&u16::try_from(USA_OFFSET).unwrap_or(0).to_le_bytes());
    r[6..8].copy_from_slice(&u16::try_from(usa_count).unwrap_or(0).to_le_bytes());
    r[0x10..0x12].copy_from_slice(&1u16.to_le_bytes()); // sequence
    r[0x12..0x14].copy_from_slice(&1u16.to_le_bytes()); // link count
    r[0x14..0x16].copy_from_slice(&u16::try_from(first_attr).unwrap_or(0).to_le_bytes());
    r[0x16..0x18].copy_from_slice(&flags.to_le_bytes());
    let mut at = first_attr;
    for a in attrs {
        r[at..at + a.len()].copy_from_slice(a);
        at += a.len();
    }
    r[at..at + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    let used = align8(at + 4);
    r[0x18..0x1C].copy_from_slice(&u32::try_from(used).unwrap_or(0).to_le_bytes());
    r[0x1C..0x20].copy_from_slice(&u32::try_from(size).unwrap_or(0).to_le_bytes());
    r[0x28..0x2A].copy_from_slice(&u16::try_from(attrs.len()).unwrap_or(0).to_le_bytes());
    r[0x2C..0x30].copy_from_slice(&entry.to_le_bytes());
    // Update sequence array: save each sector's last two bytes, then stamp the USN there.
    r[USA_OFFSET..USA_OFFSET + 2].copy_from_slice(&USN);
    for s in 0..sectors {
        let end = (s + 1) * SECTOR - 2;
        let saved = [r[end], r[end + 1]];
        let slot = USA_OFFSET + 2 + s * 2;
        r[slot..slot + 2].copy_from_slice(&saved);
        r[end..end + 2].copy_from_slice(&USN);
    }
    r
}

/// UTF-16 units of a `&str` (test convenience).
pub(crate) fn u16s(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
