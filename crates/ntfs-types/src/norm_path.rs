use crate::upcase::upcase;

/// Path comparison key: UTF-16 units upper-cased with the Windows default `$UpCase` table.
#[derive(Debug, PartialEq, Eq)]
pub struct NormPath(Box<[u16]>);

impl NormPath {
    pub fn from_units(units: &[u16]) -> Self {
        Self(units.iter().map(|&u| upcase(u)).collect())
    }

    pub fn units(&self) -> &[u16] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(s: &str) -> NormPath {
        NormPath::from_units(&s.encode_utf16().collect::<Vec<_>>())
    }

    #[test]
    fn equal_ignoring_ascii_case() {
        assert_eq!(path("a.TXT"), path("A.txt"));
        assert_ne!(path("a.txt"), path("b.txt"));
    }

    #[test]
    fn equal_ignoring_non_ascii_case() {
        for (lower, upper) in [("é", "É"), ("ω", "Ω"), ("я", "Я")] {
            assert_eq!(path(lower), path(upper), "{lower} vs {upper}");
        }
    }

    #[test]
    fn follows_windows_upcase() {
        // Windows' $UpCase leaves these alone, unlike Unicode uppercase.
        assert_ne!(path("\u{00B5}"), path("\u{039C}"), "µ vs Μ");
        assert_ne!(path("\u{0131}"), path("I"), "ı vs I");
    }

    #[test]
    fn surrogates_unchanged() {
        for units in [&[0xD800][..], &[0xDC00, 0xD800]] {
            assert_eq!(
                NormPath::from_units(units).0.as_ref(),
                units,
                "{units:04X?}"
            );
        }
    }
}
