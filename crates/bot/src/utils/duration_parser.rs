use chrono::Duration;

/// Parse a human-readable duration string into a chrono::Duration.
///
/// Supported formats:
///   "30m"      -> 30 minutes
///   "2h"       -> 2 hours
///   "1h30m"    -> 1 hour 30 minutes
///   "1h30"     -> 1 hour 30 minutes (m is optional at the end)
///   "90"       -> 90 minutes (bare number = minutes)
///
/// Returns None if the string cannot be parsed.
pub fn parse_ago_duration(input: &str) -> Option<Duration> {
    let s = input.trim().to_lowercase();
    if s.is_empty() {
        return None;
    }

    let mut total_minutes: i64 = 0;
    let mut current_number = String::new();
    let mut found_any = false;

    for ch in s.chars() {
        if ch.is_ascii_digit() {
            current_number.push(ch);
        } else if ch == 'h' {
            let hours: i64 = current_number.parse().ok()?;
            total_minutes += hours * 60;
            current_number.clear();
            found_any = true;
        } else if ch == 'm' {
            let mins: i64 = current_number.parse().ok()?;
            total_minutes += mins;
            current_number.clear();
            found_any = true;
        } else {
            return None;
        }
    }

    if !current_number.is_empty() {
        let num: i64 = current_number.parse().ok()?;
        if found_any {
            total_minutes += num;
        } else {
            total_minutes += num;
        }
    }

    if total_minutes <= 0 {
        return None;
    }

    Some(Duration::minutes(total_minutes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minutes_only() {
        assert_eq!(parse_ago_duration("30m"), Some(Duration::minutes(30)));
        assert_eq!(parse_ago_duration("90"), Some(Duration::minutes(90)));
    }

    #[test]
    fn test_hours_only() {
        assert_eq!(parse_ago_duration("2h"), Some(Duration::minutes(120)));
    }

    #[test]
    fn test_hours_and_minutes() {
        assert_eq!(parse_ago_duration("1h30m"), Some(Duration::minutes(90)));
        assert_eq!(parse_ago_duration("1h30"), Some(Duration::minutes(90)));
    }

    #[test]
    fn test_invalid() {
        assert_eq!(parse_ago_duration(""), None);
        assert_eq!(parse_ago_duration("abc"), None);
        assert_eq!(parse_ago_duration("0m"), None);
    }
}
