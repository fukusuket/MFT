/// NTFS file reference: low 48 bits = MFT entry number, high 16 bits = sequence number.
#[derive(Debug, Clone, Copy)]
pub struct FileRef(u64);

const ENTRY_BITS: u32 = 48;
const ENTRY_MASK: u64 = (1 << ENTRY_BITS) - 1;

impl FileRef {
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }

    pub const fn entry(self) -> u64 {
        self.0 & ENTRY_MASK
    }

    pub const fn sequence(self) -> u16 {
        // Bytes 6..8 are bits ENTRY_BITS..64; taking them avoids a truncating cast.
        let bytes = self.0.to_le_bytes();
        u16::from_le_bytes([bytes[6], bytes[7]])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_is_low_48_bits() {
        assert_eq!(FileRef::from_raw(0x0005_0000_0000_0023).entry(), 0x23);
    }

    #[test]
    fn sequence_is_high_16_bits() {
        assert_eq!(FileRef::from_raw(0x0005_0000_0000_0023).sequence(), 5);
    }

    // (raw, entry, sequence)
    const BOUNDARIES: [(u64, u64, u16); 4] = [
        (0, 0, 0),
        (u64::MAX, 0x0000_FFFF_FFFF_FFFF, 0xFFFF),
        (0x0000_FFFF_FFFF_FFFF, 0x0000_FFFF_FFFF_FFFF, 0),
        (0xFFFF_0000_0000_0000, 0, 0xFFFF),
    ];

    #[test]
    fn raw_round_trips() {
        for (raw, _, _) in BOUNDARIES {
            assert_eq!(FileRef::from_raw(raw).raw(), raw, "raw {raw:#018x}");
        }
    }

    #[test]
    fn entry_and_sequence_at_boundaries() {
        for (raw, entry, sequence) in BOUNDARIES {
            let r = FileRef::from_raw(raw);
            assert_eq!(
                (r.entry(), r.sequence()),
                (entry, sequence),
                "raw {raw:#018x}"
            );
        }
    }
}
