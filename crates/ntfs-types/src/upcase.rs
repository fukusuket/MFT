//! Windows default `$UpCase` table (Windows 7 and later), ADR 0009.
//!
//! Ported from ntfs-3g `libntfs-3g/unistr.c`, `ntfs_upcase_table_build()`
//! (github.com/tuxera/ntfs-3g, commit 7f0f841fc52c), GPL-2.0-or-later. See NOTICE.

use std::sync::LazyLock;

/// `(first, end_exclusive, add)`: each code point in the range maps to itself plus `add`.
const RUN: [(u16, u16, i16); 39] = [
    (0x0061, 0x007B, -32),
    (0x0451, 0x045D, -80),
    (0x1F70, 0x1F72, 74),
    (0x00E0, 0x00F7, -32),
    (0x045E, 0x0460, -80),
    (0x1F72, 0x1F76, 86),
    (0x00F8, 0x00FF, -32),
    (0x0561, 0x0587, -48),
    (0x1F76, 0x1F78, 100),
    (0x0256, 0x0258, -205),
    (0x1F00, 0x1F08, 8),
    (0x1F78, 0x1F7A, 128),
    (0x028A, 0x028C, -217),
    (0x1F10, 0x1F16, 8),
    (0x1F7A, 0x1F7C, 112),
    (0x03AC, 0x03AD, -38),
    (0x1F20, 0x1F28, 8),
    (0x1F7C, 0x1F7E, 126),
    (0x03AD, 0x03B0, -37),
    (0x1F30, 0x1F38, 8),
    (0x1FB0, 0x1FB2, 8),
    (0x03B1, 0x03C2, -32),
    (0x1F40, 0x1F46, 8),
    (0x1FD0, 0x1FD2, 8),
    (0x03C2, 0x03C3, -31),
    (0x1F51, 0x1F52, 8),
    (0x1FE0, 0x1FE2, 8),
    (0x03C3, 0x03CC, -32),
    (0x1F53, 0x1F54, 8),
    (0x1FE5, 0x1FE6, 7),
    (0x03CC, 0x03CD, -64),
    (0x1F55, 0x1F56, 8),
    (0x2170, 0x2180, -16),
    (0x03CD, 0x03CF, -63),
    (0x1F57, 0x1F58, 8),
    (0x24D0, 0x24EA, -26),
    (0x0430, 0x0450, -32),
    (0x1F60, 0x1F68, 8),
    (0xFF41, 0xFF5B, -32),
];

/// `(first, end_exclusive)`: upper/lower pairs; each odd code point maps to the even one before it.
const DUP: [(u16, u16); 24] = [
    (0x0100, 0x012F),
    (0x01A0, 0x01A6),
    (0x03E2, 0x03EF),
    (0x04CB, 0x04CC),
    (0x0132, 0x0137),
    (0x01B3, 0x01B7),
    (0x0460, 0x0481),
    (0x04D0, 0x04EB),
    (0x0139, 0x0149),
    (0x01CD, 0x01DD),
    (0x0490, 0x04BF),
    (0x04EE, 0x04F5),
    (0x014A, 0x0178),
    (0x01DE, 0x01EF),
    (0x04BF, 0x04BF),
    (0x04F8, 0x04F9),
    (0x0179, 0x017E),
    (0x01F4, 0x01F5),
    (0x04C1, 0x04C4),
    (0x1E00, 0x1E95),
    (0x018B, 0x018B),
    (0x01FA, 0x0218),
    (0x04C7, 0x04C8),
    (0x1EA0, 0x1EF9),
];

/// `(code point, upper case)`: single mappings.
const BYTE: [(u16, u16); 31] = [
    (0x00FF, 0x0178),
    (0x01AD, 0x01AC),
    (0x01F3, 0x01F1),
    (0x0269, 0x0196),
    (0x0183, 0x0182),
    (0x01B0, 0x01AF),
    (0x0253, 0x0181),
    (0x026F, 0x019C),
    (0x0185, 0x0184),
    (0x01B9, 0x01B8),
    (0x0254, 0x0186),
    (0x0272, 0x019D),
    (0x0188, 0x0187),
    (0x01BD, 0x01BC),
    (0x0259, 0x018F),
    (0x0275, 0x019F),
    (0x018C, 0x018B),
    (0x01C6, 0x01C4),
    (0x025B, 0x0190),
    (0x0283, 0x01A9),
    (0x0192, 0x0191),
    (0x01C9, 0x01C7),
    (0x0260, 0x0193),
    (0x0288, 0x01AE),
    (0x0199, 0x0198),
    (0x01CC, 0x01CA),
    (0x0263, 0x0194),
    (0x0292, 0x01B7),
    (0x01A8, 0x01A7),
    (0x01DD, 0x018E),
    (0x0268, 0x0197),
];

/// Changes from Windows Vista and 7: `(first, last_inclusive, add, step)`.
const DELTA: [(u16, u16, i16, u8); 64] = [
    (0x037B, 0x037D, 130, 1),
    (0x1F80, 0x1F87, 8, 1),
    (0x1F90, 0x1F97, 8, 1),
    (0x1FA0, 0x1FA7, 8, 1),
    (0x2C30, 0x2C5E, -48, 1),
    (0x2D00, 0x2D25, -7264, 1),
    (0x2C68, 0x2C6C, -1, 2),
    (0x0219, 0x021F, -1, 2),
    (0x0223, 0x0233, -1, 2),
    (0x0247, 0x024F, -1, 2),
    (0x03D9, 0x03E1, -1, 2),
    (0x048B, 0x048F, -1, 2),
    (0x04FB, 0x0513, -1, 2),
    (0x2C81, 0x2CE3, -1, 2),
    (0x03F8, 0x03FB, -1, 3),
    (0x04C6, 0x04CE, -1, 4),
    (0x023C, 0x0242, -1, 6),
    (0x04ED, 0x04F7, -1, 10),
    (0x0450, 0x045D, -80, 13),
    (0x2C61, 0x2C76, -1, 21),
    (0x1FCC, 0x1FFC, -9, 48),
    (0x0180, 0x0180, 195, 1),
    (0x0195, 0x0195, 97, 1),
    (0x019A, 0x019A, 163, 1),
    (0x019E, 0x019E, 130, 1),
    (0x01BF, 0x01BF, 56, 1),
    (0x01F9, 0x01F9, -1, 1),
    (0x023A, 0x023A, 10795, 1),
    (0x023E, 0x023E, 10792, 1),
    (0x026B, 0x026B, 10743, 1),
    (0x027D, 0x027D, 10727, 1),
    (0x0280, 0x0280, -218, 1),
    (0x0289, 0x0289, -69, 1),
    (0x028C, 0x028C, -71, 1),
    (0x03F2, 0x03F2, 7, 1),
    (0x04CF, 0x04CF, -15, 1),
    (0x1D7D, 0x1D7D, 3814, 1),
    (0x1FB3, 0x1FB3, 9, 1),
    (0x214E, 0x214E, -28, 1),
    (0x2184, 0x2184, -1, 1),
    (0x023A, 0x023E, 0, 4),
    (0x0250, 0x0250, 10783, 2),
    (0x0251, 0x0251, 10780, 2),
    (0x0271, 0x0271, 10749, 2),
    (0x0371, 0x0373, -1, 2),
    (0x0377, 0x0377, -1, 2),
    (0x03C2, 0x03C2, 0, 2),
    (0x03D7, 0x03D7, -8, 2),
    (0x0515, 0x0523, -1, 2),
    (0x1D79, 0x1D79, -30204, 2),
    (0x1EFB, 0x1EFF, -1, 2),
    (0x1FC3, 0x1FF3, 9, 48),
    (0x1FCC, 0x1FFC, 0, 48),
    (0x2C65, 0x2C65, -10795, 2),
    (0x2C66, 0x2C66, -10792, 2),
    (0x2C73, 0x2C73, -1, 2),
    (0xA641, 0xA65F, -1, 2),
    (0xA663, 0xA66D, -1, 2),
    (0xA681, 0xA697, -1, 2),
    (0xA723, 0xA72F, -1, 2),
    (0xA733, 0xA76F, -1, 2),
    (0xA77A, 0xA77C, -1, 2),
    (0xA77F, 0xA787, -1, 2),
    (0xA78C, 0xA78C, -1, 2),
];

/// One entry per `u16`, so `usize::from(u16)` is always in bounds.
type Table = [u16; 1 << 16];

static UPCASE: LazyLock<Box<Table>> = LazyLock::new(build);

/// Upper case of one UTF-16 code unit as Windows' default `$UpCase` defines it.
pub(crate) fn upcase(u: u16) -> u16 {
    UPCASE[usize::from(u)]
}

fn build() -> Box<Table> {
    let mut table = Box::new([0; 1 << 16]);
    for (u, slot) in (0..=u16::MAX).zip(table.iter_mut()) {
        *slot = u;
    }
    for &(first, end, add) in &RUN {
        for u in first..end {
            set(&mut table, u, u.wrapping_add_signed(add));
        }
    }
    for &(first, end) in &DUP {
        for u in (first..end).step_by(2) {
            set(&mut table, u.wrapping_add(1), u);
        }
    }
    for &(u, upper) in &BYTE {
        set(&mut table, u, upper);
    }
    for &(first, last, add, step) in &DELTA {
        for u in (first..=last).step_by(usize::from(step)) {
            set(&mut table, u, u.wrapping_add_signed(add));
        }
    }
    table
}

fn set(table: &mut Table, at: u16, value: u16) {
    table[usize::from(at)] = value;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FNV-1a-64 over the little-endian table. The expected value comes from a rebuild of the
    /// ntfs-3g tables that reproduces ntfs-3g's documented Win7/8 `$UpCase` MD5 (ADR 0009).
    #[test]
    fn table_matches_windows_7_and_later() {
        let hash = (0..=u16::MAX)
            .flat_map(|u| upcase(u).to_le_bytes())
            .fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
                (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
            });
        assert_eq!(hash, 0x48ed_4531_e939_9927);
    }
}
