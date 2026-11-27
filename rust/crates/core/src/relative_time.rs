//! Luxon's `DateTime.fromISO(iso).toRelative({ style: 'short' })`, which both
//! references use for a listing's age ("5 min. ago"), in English (en-US).
//!
//! Luxon picks the largest of years, months, days, hours, minutes and seconds
//! with a whole count, truncated toward zero; years and months count calendar
//! steps. It works in the system's time zone; this works in UTC, which only
//! differs for month and year counts near a month boundary.

/// Milliseconds since the Unix epoch.
pub type EpochMillis = i64;

/// Days since 1970-01-01 of a civil date (Howard Hinnant's algorithm).
#[expect(clippy::arithmetic_side_effects, reason = "civil-date arithmetic on epoch years stays far inside i64")]
const fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil date of a day since 1970-01-01.
#[expect(clippy::arithmetic_side_effects, reason = "civil-date arithmetic on epoch years stays far inside i64")]
const fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[expect(clippy::arithmetic_side_effects, reason = "civil-date arithmetic on epoch years stays far inside i64")]
const fn days_in_month(y: i64, m: i64) -> i64 {
    days_from_civil(if m == 12 { y + 1 } else { y }, if m == 12 { 1 } else { m + 1 }, 1) - days_from_civil(y, m, 1)
}

const DAY: i64 = 86_400_000;

/// `ms` plus `months` calendar months, the day clamped to the month's length.
#[expect(clippy::arithmetic_side_effects, reason = "month arithmetic on epoch timestamps stays far inside i64")]
fn plus_months(ms: EpochMillis, months: i64) -> EpochMillis {
    let days = ms.div_euclid(DAY);
    let time = ms.rem_euclid(DAY);
    let (y, m, d) = civil_from_days(days);
    let total = y * 12 + (m - 1) + months;
    let (y, m) = (total.div_euclid(12), total.rem_euclid(12) + 1);
    days_from_civil(y, m, d.min(days_in_month(y, m))) * DAY + time
}

/// Parses an ISO 8601 timestamp (`2024-11-21T13:02:11Z`, with optional
/// fraction and `±HH:MM` offset; no offset is read as UTC).
pub fn parse_iso(text: &str) -> Option<EpochMillis> {
    use time::format_description::well_known::Iso8601;
    use time::{PrimitiveDateTime, UtcOffset};

    // Luxon allows a compact offset on an extended calendar date. Parsing the
    // two pieces separately lets each use its own ISO representation.
    let (_, clock) = text.split_once('T')?;
    let zone = clock.find(['+', '-', 'Z']).map(|at| text.len().saturating_sub(clock.len()).saturating_add(at));
    let (local, offset) = match zone {
        Some(at) => {
            let (local, zone) = text.split_at(at);
            (local, UtcOffset::parse(zone, &Iso8601::DEFAULT).ok()?)
        }
        None => (text, UtcOffset::UTC),
    };
    let instant = PrimitiveDateTime::parse(local, &Iso8601::DEFAULT).ok()?.assume_offset(offset);
    instant.unix_timestamp_nanos().div_euclid(1_000_000).try_into().ok()
}

/// Whole calendar steps of `months` from `start` to `end` (`end >= start`).
#[expect(clippy::arithmetic_side_effects, reason = "month arithmetic on epoch timestamps stays far inside i64")]
fn whole_months(start: EpochMillis, end: EpochMillis, step: i64) -> i64 {
    let mut n = 0;
    while plus_months(start, (n + 1) * step) <= end {
        n += 1;
    }
    n
}

/// `Intl.RelativeTimeFormat('en', { style: 'short', numeric: 'always' })`.
fn format(count: i64, negative: bool, unit: &str) -> String {
    let plural = count.abs() != 1;
    let label = match unit {
        "years" => "yr.",
        "months" => "mo.",
        "days" if plural => "days",
        "days" => "day",
        "hours" => "hr.",
        "minutes" => "min.",
        _ => "sec.",
    };
    let n = group(count.abs());
    if negative { format!("{n} {label} ago") } else { format!("in {n} {label}") }
}

/// `1234` as `1,234`.
fn group(n: i64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && digits.len().saturating_sub(i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `DateTime.fromMillis(time).toRelative({ style: 'short', base: now })`.
pub fn to_relative_short(time: EpochMillis, now: EpochMillis) -> String {
    // Luxon diffs `time` against `now`; past times count negative.
    let negative = time < now;
    let (start, end) = if negative { (time, now) } else { (now, time) };
    let years = whole_months(start, end, 12);
    if years >= 1 {
        return format(years, negative, "years");
    }
    let months = whole_months(start, end, 1);
    if months >= 1 {
        return format(months, negative, "months");
    }
    let ms = end.saturating_sub(start);
    for (unit, size) in [("days", DAY), ("hours", 3_600_000), ("minutes", 60_000), ("seconds", 1000)] {
        if ms >= size {
            return format(ms.div_euclid(size), negative, unit);
        }
    }
    // `format(start > end ? -0 : 0, 'seconds')`: "0 sec. ago" or "in 0 sec."
    format(0, negative, "seconds")
}

/// `DateTime.fromISO(iso).toRelative({ style: 'short' })` at `now`; Luxon
/// returns `null` for an invalid date, which upstream turns into `''`.
pub fn iso_to_relative_short(iso: &str, now: EpochMillis) -> Option<String> {
    parse_iso(iso).map(|t| to_relative_short(t, now))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_iso_timestamps() {
        assert_eq!(parse_iso("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso("2024-02-29T12:00:00.5Z"), Some(1_709_208_000_500));
        assert_eq!(parse_iso("2024-02-29T13:00:00+01:00"), Some(1_709_208_000_000));
        assert_eq!(parse_iso("1970-01-01T03:00:00+0130"), Some(5_400_000));
        assert_eq!(parse_iso("1970-01-01T03:00:00+01:30"), Some(5_400_000));
        assert_eq!(parse_iso("1970-01-01T00:00"), Some(0));
        for invalid in ["2023-02-29T00:00:00Z", "2024-02-29T28:00:00Z", "1970-01-01T00:60:00Z", "nonsense"] {
            assert_eq!(parse_iso(invalid), None);
        }
    }

    #[test]
    fn picks_the_largest_whole_unit() {
        let now = parse_iso("2024-03-31T12:00:00Z").unwrap();
        let ago = |s: &str| to_relative_short(parse_iso(s).unwrap(), now);
        assert_eq!(ago("2024-03-31T11:59:30Z"), "30 sec. ago");
        assert_eq!(ago("2024-03-31T11:55:01Z"), "4 min. ago");
        assert_eq!(ago("2024-03-31T09:00:00Z"), "3 hr. ago");
        assert_eq!(ago("2024-03-30T12:00:00Z"), "1 day ago");
        assert_eq!(ago("2024-03-02T12:00:00Z"), "29 days ago");
        // Feb 29 plus one month is Mar 29.
        assert_eq!(ago("2024-02-29T12:00:00Z"), "1 mo. ago");
        assert_eq!(ago("2022-03-31T12:00:00Z"), "2 yr. ago");
        assert_eq!(ago("2024-03-31T12:00:00Z"), "in 0 sec.");
        assert_eq!(ago("2024-03-31T11:59:59.5Z"), "0 sec. ago");
        assert_eq!(to_relative_short(now + 90_000, now), "in 1 min.");
        assert_eq!(group(1_234_567), "1,234,567");
    }
}
