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

    /// The UTC time at `date` (year, month, day) and `time` (hours, minutes, seconds); `None`
    /// if there is no such time or it is before 1601.
    pub fn from_utc(date: (u64, u64, u64), time: (u64, u64, u64)) -> Option<Self> {
        let (year, month, day) = date;
        let (h, m, sec) = time;
        if year < 1601 || !(1..=12).contains(&month) || h > 23 || m > 59 || sec > 59 {
            return None;
        }
        let days = days_from_civil(year, month, day)?;
        let secs = days
            .checked_mul(86_400)?
            .checked_add(h * 3600 + m * 60 + sec)?;
        Some(Self(secs.checked_mul(10_000_000)?))
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

/// Day count since 1601-01-01 for a Gregorian date from 1601 on (H. Hinnant's
/// `days_from_civil`, counted from 1600-03-01); `None` if the day is not in the month.
fn days_from_civil(year: u64, month: u64, day: u64) -> Option<u64> {
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days_in_month = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if !(1..=days_in_month).contains(&day) {
        return None;
    }
    let y = year - 1600 - u64::from(month <= 2);
    let (era, yoe) = (y / 400, y % 400);
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era.checked_mul(146_097)?.checked_add(doe)?.checked_sub(306)
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
    fn from_utc_builds_the_time_iso8601_shows() {
        let cases = [
            ((1601, 1, 1), (0, 0, 0), "1601-01-01T00:00:00.0000000Z"),
            ((2000, 2, 29), (0, 0, 0), "2000-02-29T00:00:00.0000000Z"),
            ((2024, 2, 29), (12, 34, 56), "2024-02-29T12:34:56.0000000Z"),
            ((2026, 10, 2), (23, 59, 59), "2026-10-02T23:59:59.0000000Z"),
        ];
        for (date, time, text) in cases {
            assert_eq!(
                Filetime::from_utc(date, time).map(Filetime::iso8601),
                Some(text.to_string()),
                "{date:?} {time:?}"
            );
        }
    }

    #[test]
    fn from_utc_rejects_times_that_do_not_exist() {
        let cases = [
            ((1600, 12, 31), (23, 59, 59)),
            ((2026, 2, 29), (0, 0, 0)),
            ((1900, 2, 29), (0, 0, 0)),
            ((2026, 2, 30), (0, 0, 0)),
            ((2026, 4, 31), (0, 0, 0)),
            ((2026, 13, 1), (0, 0, 0)),
            ((2026, 0, 1), (0, 0, 0)),
            ((2026, 1, 0), (0, 0, 0)),
            ((2026, 1, 1), (24, 0, 0)),
            ((2026, 1, 1), (0, 60, 0)),
            ((2026, 1, 1), (0, 0, 60)),
            ((u64::MAX, 1, 1), (0, 0, 0)),
        ];
        for (date, time) in cases {
            assert_eq!(Filetime::from_utc(date, time), None, "{date:?} {time:?}");
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
