/// Windows FILETIME: 100 ns intervals since 1601-01-01 00:00:00 UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Filetime(u64);

impl Filetime {
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_round_trips() {
        for raw in [0, 1, u64::MAX] {
            assert_eq!(Filetime::from_raw(raw).raw(), raw, "raw {raw:#018x}");
        }
    }

    #[test]
    fn orders_chronologically() {
        assert!(Filetime::from_raw(0) < Filetime::from_raw(1));
        assert!(Filetime::from_raw(1) < Filetime::from_raw(u64::MAX));
        assert!(Filetime::from_raw(7) == Filetime::from_raw(7));
    }
}
