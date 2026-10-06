use ntfs_types::Filetime;

/// `YYYY-MM-DDTHH:MM:SS.fffffffZ`, UTC, full 100 ns precision (ADR 0011).
pub(crate) fn iso8601(time: Filetime) -> String {
    let ticks = time.raw();
    let (secs, fraction) = (ticks / 10_000_000, ticks % 10_000_000);
    let (days, secs_of_day) = (secs / 86_400, secs % 86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{fraction:07}Z",
        secs_of_day / 3600,
        secs_of_day / 60 % 60,
        secs_of_day % 60
    )
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
    fn formats_utc_with_100ns_precision() {
        let cases = [
            (0, "1601-01-01T00:00:00.0000000Z"),
            (133_444_555_666_777_888, "2023-11-14T17:12:46.6777888Z"),
            (133_536_836_961_234_567, "2024-02-29T12:34:56.1234567Z"),
            (125_962_560_001_234_567, "2000-02-29T00:00:00.1234567Z"),
            (u64::MAX, "60056-05-28T05:36:10.9551615Z"),
        ];
        for (raw, text) in cases {
            assert_eq!(iso8601(Filetime::from_raw(raw)), text, "raw {raw}");
        }
    }
}
