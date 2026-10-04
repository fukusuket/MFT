use std::fmt::{self, Write as _};

/// File name as stored by NTFS: raw UTF-16 code units, possibly ill-formed.
#[derive(Debug)]
pub struct NtfsName(Box<[u16]>);

impl NtfsName {
    pub fn from_units(units: &[u16]) -> Self {
        Self(units.into())
    }

    pub fn units(&self) -> &[u16] {
        &self.0
    }
}

/// Lossless and unambiguous: valid UTF-16 as-is, `\` as `\\`, an unpaired surrogate as `\u{D800}`.
/// Control characters are not escaped here; output layers handle them (ADR 0002 #4).
impl fmt::Display for NtfsName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for decoded in char::decode_utf16(self.0.iter().copied()) {
            match decoded {
                Ok('\\') => f.write_str(r"\\")?,
                Ok(c) => f.write_char(c)?,
                Err(e) => write!(f, r"\u{{{:04X}}}", e.unpaired_surrogate())?,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn name(s: &str) -> NtfsName {
        NtfsName::from_units(&s.encode_utf16().collect::<Vec<_>>())
    }

    #[test]
    fn displays_valid_utf16_unchanged() {
        for s in ["a.txt", "日本語.txt", "😀"] {
            assert_eq!(name(s).to_string(), s);
        }
    }

    #[test]
    fn escapes_backslash() {
        assert_eq!(name(r"a\b").to_string(), r"a\\b");
    }

    #[test]
    fn escapes_unpaired_surrogates() {
        let cases: [(&[u16], &str); 3] = [
            (&[0x61, 0xD800, 0x62], r"a\u{D800}b"),
            (&[0xDC00], r"\u{DC00}"),
            (&[0xDC00, 0xD800], r"\u{DC00}\u{D800}"),
        ];
        for (units, shown) in cases {
            assert_eq!(
                NtfsName::from_units(units).to_string(),
                shown,
                "{units:04X?}"
            );
        }
    }

    /// Inverse of `Display`, test-only: proves the escaping loses nothing and is unambiguous.
    fn unescape(shown: &str) -> Option<Vec<u16>> {
        let mut units = Vec::new();
        let mut chars = shown.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                units.extend(c.encode_utf16(&mut [0; 2]).iter());
                continue;
            }
            match chars.next()? {
                '\\' => units.push(u16::from(b'\\')),
                'u' => {
                    if chars.next()? != '{' {
                        return None;
                    }
                    let hex: String = chars.by_ref().take_while(|&h| h != '}').collect();
                    units.push(u16::from_str_radix(&hex, 16).ok()?);
                }
                _ => return None,
            }
        }
        Some(units)
    }

    /// Code units biased toward the characters that make escaping ambiguous.
    fn tricky_unit() -> impl Strategy<Value = u16> {
        prop_oneof![
            any::<u16>(),
            0xD800u16..=0xDFFF,
            proptest::sample::select(b"\\u{}0123456789ABCDEF".map(u16::from).to_vec()),
        ]
    }

    proptest! {
        #[test]
        fn display_is_unambiguous(units in proptest::collection::vec(tricky_unit(), 0..32)) {
            let shown = NtfsName::from_units(&units).to_string();
            prop_assert_eq!(unescape(&shown), Some(units));
        }

        #[test]
        fn units_round_trip(units in proptest::collection::vec(any::<u16>(), 0..64)) {
            let name = NtfsName::from_units(&units);
            prop_assert_eq!(name.units(), units.as_slice());
        }
    }
}
