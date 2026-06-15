use chrono::Duration;

const SECOND: i64 = 1;
const MINUTE: i64 = 60;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;
const WEEK: i64 = 7 * DAY;
const MONTH: i64 = 30 * DAY;
const YEAR: i64 = 365 * DAY;

/// Parse a human readable duration string into a chrono::Duration.
///
/// Accepts a sequence of `<number><unit>` pairs. Whitespace, commas and `and`
/// between pairs are ignored. Units are case-insensitive and may be:
///
/// - seconds: `s`, `sec`, `secs`, `second`, `seconds`
/// - minutes: `m`, `min`, `mins`, `minute`, `minutes`
/// - hours:   `h`, `hr`, `hrs`, `hour`, `hours`
/// - days:    `d`, `day`, `days`
/// - weeks:   `w`, `wk`, `wks`, `week`, `weeks`
/// - months:  `mo`, `mon`, `mos`, `month`, `months` (30 days)
/// - years:   `y`, `yr`, `yrs`, `year`, `years` (365 days)
///
/// A trailing bare number is interpreted as minutes for backwards
/// compatibility (e.g. `1h30` is the same as `1h30m`).
///
/// Returns None if the string cannot be parsed or evaluates to zero.
pub fn parse_ago_duration(input: &str) -> Option<Duration> {
    let s = input.trim().to_lowercase();
    if s.is_empty() {
        return None;
    }

    let bytes = s.as_bytes();
    let mut i = 0;
    let mut total_seconds: i64 = 0;

    while i < bytes.len() {
        while i < bytes.len() && is_separator(bytes[i]) {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        let digit_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == digit_start {
            return None;
        }
        let num: i64 = s[digit_start..i].parse().ok()?;

        let unit_start = i;
        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
            i += 1;
        }
        let unit = &s[unit_start..i];

        let unit_seconds = unit_to_seconds(unit)?;
        let delta = num.checked_mul(unit_seconds)?;
        total_seconds = total_seconds.checked_add(delta)?;
    }

    if total_seconds <= 0 {
        return None;
    }

    Some(Duration::seconds(total_seconds))
}

fn is_separator(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b',')
}

fn unit_to_seconds(unit: &str) -> Option<i64> {
    match unit {
        "" => Some(MINUTE),
        "s" | "sec" | "secs" | "second" | "seconds" => Some(SECOND),
        "m" | "min" | "mins" | "minute" | "minutes" => Some(MINUTE),
        "h" | "hr" | "hrs" | "hour" | "hours" => Some(HOUR),
        "d" | "day" | "days" => Some(DAY),
        "w" | "wk" | "wks" | "week" | "weeks" => Some(WEEK),
        "mo" | "mon" | "mos" | "month" | "months" => Some(MONTH),
        "y" | "yr" | "yrs" | "year" | "years" => Some(YEAR),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minutes_only() {
        assert_eq!(parse_ago_duration("30m"), Some(Duration::minutes(30)));
        assert_eq!(parse_ago_duration("90"), Some(Duration::minutes(90)));
        assert_eq!(parse_ago_duration("5min"), Some(Duration::minutes(5)));
        assert_eq!(parse_ago_duration("5mins"), Some(Duration::minutes(5)));
        assert_eq!(parse_ago_duration("5minutes"), Some(Duration::minutes(5)));
    }

    #[test]
    fn test_seconds() {
        assert_eq!(parse_ago_duration("45s"), Some(Duration::seconds(45)));
        assert_eq!(parse_ago_duration("45sec"), Some(Duration::seconds(45)));
        assert_eq!(parse_ago_duration("45secs"), Some(Duration::seconds(45)));
        assert_eq!(parse_ago_duration("45seconds"), Some(Duration::seconds(45)));
    }

    #[test]
    fn test_hours_only() {
        assert_eq!(parse_ago_duration("2h"), Some(Duration::hours(2)));
        assert_eq!(parse_ago_duration("2hr"), Some(Duration::hours(2)));
        assert_eq!(parse_ago_duration("2hrs"), Some(Duration::hours(2)));
        assert_eq!(parse_ago_duration("2hours"), Some(Duration::hours(2)));
    }

    #[test]
    fn test_days_weeks() {
        assert_eq!(parse_ago_duration("3d"), Some(Duration::days(3)));
        assert_eq!(parse_ago_duration("3days"), Some(Duration::days(3)));
        assert_eq!(parse_ago_duration("2w"), Some(Duration::weeks(2)));
        assert_eq!(parse_ago_duration("2weeks"), Some(Duration::weeks(2)));
    }

    #[test]
    fn test_months_years() {
        assert_eq!(parse_ago_duration("1mo"), Some(Duration::days(30)));
        assert_eq!(parse_ago_duration("1month"), Some(Duration::days(30)));
        assert_eq!(parse_ago_duration("6months"), Some(Duration::days(180)));
        assert_eq!(parse_ago_duration("1y"), Some(Duration::days(365)));
        assert_eq!(parse_ago_duration("1year"), Some(Duration::days(365)));
        assert_eq!(parse_ago_duration("2yrs"), Some(Duration::days(730)));
    }

    #[test]
    fn test_hours_and_minutes() {
        assert_eq!(parse_ago_duration("1h30m"), Some(Duration::minutes(90)));
        assert_eq!(parse_ago_duration("1h30"), Some(Duration::minutes(90)));
        assert_eq!(parse_ago_duration("1hr 30min"), Some(Duration::minutes(90)));
    }

    #[test]
    fn test_compound() {
        let expected = Duration::days(365) + Duration::days(60) + Duration::days(3);
        assert_eq!(parse_ago_duration("1y2mo3d"), Some(expected));
        assert_eq!(
            parse_ago_duration("1y, 2mo, 3d"),
            Some(expected)
        );
    }

    #[test]
    fn test_case_insensitive() {
        assert_eq!(parse_ago_duration("2H"), Some(Duration::hours(2)));
        assert_eq!(parse_ago_duration("1Day"), Some(Duration::days(1)));
        assert_eq!(parse_ago_duration("1MONTH"), Some(Duration::days(30)));
    }

    #[test]
    fn test_invalid() {
        assert_eq!(parse_ago_duration(""), None);
        assert_eq!(parse_ago_duration("abc"), None);
        assert_eq!(parse_ago_duration("0m"), None);
        assert_eq!(parse_ago_duration("5x"), None);
        assert_eq!(parse_ago_duration("m"), None);
    }
}
