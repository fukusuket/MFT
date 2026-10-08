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

    /// `YYYY-MM-DDTHH:MM:SS.fffffffZ`, UTC, full 100 ns precision (ADR 0011).
    pub fn iso8601(self) -> String {
        let (date, (h, m, sec), fraction) = self.parts();
        format!("{date}T{h:02}:{m:02}:{sec:02}.{fraction:07}Z")
    }

    /// `YYYY-MM-DD HH:MM:SS.fff`, UTC, truncated to milliseconds: Sysmon's `CreationUtcTime`.
    pub fn sysmon(self) -> String {
        let (date, (h, m, sec), fraction) = self.parts();
        format!("{date} {h:02}:{m:02}:{sec:02}.{:03}", fraction / 10_000)
    }

    /// `YYYY-MM-DD`, (hours, minutes, seconds) and the 100 ns fraction of the second.
    fn parts(self) -> (String, (u64, u64, u64), u64) {
        let (secs, fraction) = (self.0 / 10_000_000, self.0 % 10_000_000);
        let (days, secs_of_day) = (secs / 86_400, secs % 86_400);
        let (year, month, day) = civil_from_days(days);
        let time = (secs_of_day / 3600, secs_of_day / 60 % 60, secs_of_day % 60);
        (format!("{year:04}-{month:02}-{day:02}"), time, fraction)
    }
}

/// Gregorian date for a day count since 1601-01-01 (H. Hinnant's `civil_from_days`,
/// counted from 1600-03-01 so everything stays unsigned).
fn civil_from_days(days_since_1601: u64) -> (u64, u64, u64) {
    let n = days_since_1601 + 306; // 1600-03-01 .. 1601-01-01
    let (era, doe) = (n / 146_097, n % 146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = 1600 + era * 400 + yoe + u64::from(month <= 2);
    (year, month, day)
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

    #[test]
    fn formats_utc_with_100ns_precision() {
        let cases = [
            (0, "1601-01-01T00:00:00.0000000Z"),
            (133_444_555_666_777_888, "2023-11-14T17:12:46.6777888Z"),
            (133_536_836_961_234_567, "2024-02-29T12:34:56.1234567Z"),
            (125_962_560_001_234_567, "2000-02-29T00:00:00.1234567Z"),
            (u64::MAX, "60056-05-28T05:36:10.9551615Z"),
        ];
        for (raw, text) in cases {
            assert_eq!(Filetime::from_raw(raw).iso8601(), text, "raw {raw}");
        }
    }

    #[test]
    fn sysmon_format_truncates_to_milliseconds() {
        let cases = [
            (0, "1601-01-01 00:00:00.000"),
            (133_444_555_666_777_888, "2023-11-14 17:12:46.677"),
            (133_536_836_961_234_567, "2024-02-29 12:34:56.123"),
        ];
        for (raw, text) in cases {
            assert_eq!(Filetime::from_raw(raw).sysmon(), text, "raw {raw}");
        }
    }
}
