use chrono::{DateTime, Datelike, Duration, Month, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;

/// The rolling periods offered first, in days.
const PRESET_DAYS: [i64; 5] = [1, 7, 14, 30, 90];

/// Longest rolling period accepted, in days.
const MAX_DAYS: i64 = 90;

/// How many months back the suggestions reach.
const SUGGESTED_MONTHS: u32 = 120;

/// Discord shows at most 25 autocomplete choices.
const MAX_SUGGESTIONS: usize = 25;

/// What to tell someone whose period could not be read by [`parse`].
pub const INVALID_PERIOD_HELP: &str = "Pick a period from the list, or type one like `Last 7 days`, `45d`, `July 2026` or `2026-07`. Periods go up to 90 days, and months can't be in the future.";

/// A stretch of time to summarize: the last few days, or a calendar month.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    LastDays(i64),
    Month { year: i32, month: u32 },
}

impl Period {
    /// How the period reads, e.g. "Last 7 days" or "July 2026". [`parse`]
    /// reads this back.
    pub fn label(self) -> String {
        match self {
            Self::LastDays(1) => "Last 24 hours".to_string(),
            Self::LastDays(days) => format!("Last {days} days"),
            Self::Month { year, month } => format!("{} {year}", month_name(month)),
        }
    }

    /// When the period starts and ends. A month runs from midnight on its
    /// first day to midnight on the next month's first, in `tz`. Nothing
    /// reaches past `now`.
    pub fn bounds(self, now: DateTime<Utc>, tz: Tz) -> (DateTime<Utc>, DateTime<Utc>) {
        match self {
            Self::LastDays(days) => (now - Duration::days(days), now),
            Self::Month { year, month } => {
                let (next_year, next_month) = if month == 12 {
                    (year + 1, 1)
                } else {
                    (year, month + 1)
                };
                let start = month_start(year, month, tz);
                let end = month_start(next_year, next_month, tz);
                (start.min(now), end.min(now))
            }
        }
    }

    /// The bounds of the period just before this one, to compare against: the
    /// same number of days again, or the whole calendar month before.
    pub fn previous_bounds(self, now: DateTime<Utc>, tz: Tz) -> (DateTime<Utc>, DateTime<Utc>) {
        match self {
            Self::LastDays(days) => (now - Duration::days(2 * days), now - Duration::days(days)),
            Self::Month { year, month } => {
                let (previous_year, previous_month) = if month == 1 {
                    (year - 1, 12)
                } else {
                    (year, month - 1)
                };
                let start = month_start(previous_year, previous_month, tz);
                let end = month_start(year, month, tz);
                (start.min(now), end.min(now))
            }
        }
    }
}

/// Midnight on the first day of a month in `tz`, as a UTC instant.
fn month_start(year: i32, month: u32, tz: Tz) -> DateTime<Utc> {
    let midnight = NaiveDate::from_ymd_opt(year, month, 1)
        .unwrap_or_default()
        .and_hms_opt(0, 0, 0)
        .unwrap_or_default();
    // A timezone can skip midnight on a DST change; fall back to UTC's.
    tz.from_local_datetime(&midnight)
        .earliest()
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|| midnight.and_utc())
}

fn month_name(month: u32) -> &'static str {
    u8::try_from(month)
        .ok()
        .and_then(|m| Month::try_from(m).ok())
        .map(|m| m.name())
        .unwrap_or("?")
}

/// Autocomplete choices for what the user has typed so far: the rolling
/// periods, then calendar months from the current one back.
pub fn suggestions(partial: &str, today: NaiveDate) -> Vec<String> {
    let needle = partial.trim().to_lowercase();

    let presets = PRESET_DAYS.into_iter().map(Period::LastDays);
    let months = (0..SUGGESTED_MONTHS).map(|back| {
        let index = today.year() * 12 + today.month0() as i32 - back as i32;
        Period::Month {
            year: index.div_euclid(12),
            month: index.rem_euclid(12) as u32 + 1,
        }
    });

    presets
        .chain(months)
        .map(Period::label)
        .filter(|label| label.to_lowercase().contains(&needle))
        .take(MAX_SUGGESTIONS)
        .collect()
}

/// Reads a period as picked from [`suggestions`] or typed by hand:
/// "Last 7 days", "7d", "24h", "July 2026", "jul 2026", "2026-07", "July"
/// (the most recent one), "this month" or "last month".
///
/// `None` for anything else, for months that have not started yet, and for
/// rolling periods outside 1 to [`MAX_DAYS`] days.
pub fn parse(input: &str, today: NaiveDate) -> Option<Period> {
    let text = input.trim().to_lowercase();
    let this_month = (today.year(), today.month());

    let month = |year: i32, month: u32| {
        let valid = (1..=12).contains(&month) && year >= 2000 && (year, month) <= this_month;
        valid.then_some(Period::Month { year, month })
    };

    match text.as_str() {
        "24h" | "last 24 hours" | "last 24h" => return Some(Period::LastDays(1)),
        "this month" => return month(this_month.0, this_month.1),
        "last month" => {
            return if this_month.1 == 1 {
                month(this_month.0 - 1, 12)
            } else {
                month(this_month.0, this_month.1 - 1)
            };
        }
        _ => {}
    }

    // "last 7 days", "7 days", "7d"
    let days_text = text.strip_prefix("last ").unwrap_or(&text);
    if let Some(number) = ["days", "day", "d"]
        .iter()
        .find_map(|suffix| days_text.strip_suffix(suffix))
        && let Ok(days) = number.trim().parse::<i64>()
    {
        return (1..=MAX_DAYS)
            .contains(&days)
            .then_some(Period::LastDays(days));
    }

    // "2026-07"
    if let Some((year, number)) = text.split_once('-')
        && let (Ok(year), Ok(number)) = (year.parse::<i32>(), number.parse::<u32>())
    {
        return month(year, number);
    }

    // "july 2026", "jul 2026", "july"
    let mut words = text.split_whitespace();
    let number = words.next()?.parse::<Month>().ok()?.number_from_month();
    let period = match words.next() {
        Some(year) => month(year.parse().ok()?, number),
        // A bare month name is the most recent one.
        None if number <= this_month.1 => month(this_month.0, number),
        None => month(this_month.0 - 1, number),
    };
    if words.next().is_some() {
        return None;
    }
    period
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()
    }

    fn month(year: i32, month: u32) -> Option<Period> {
        Some(Period::Month { year, month })
    }

    #[test]
    fn suggests_presets_then_months() {
        let all = suggestions("", today());
        assert_eq!(all.len(), 25);
        assert_eq!(
            &all[..7],
            [
                "Last 24 hours",
                "Last 7 days",
                "Last 14 days",
                "Last 30 days",
                "Last 90 days",
                "October 2026",
                "September 2026"
            ]
        );
    }

    #[test]
    fn suggestions_narrow_as_you_type() {
        assert_eq!(
            suggestions("July 202", today()),
            [
                "July 2026",
                "July 2025",
                "July 2024",
                "July 2023",
                "July 2022",
                "July 2021",
                "July 2020"
            ]
        );
        assert_eq!(suggestions("last 1", today()), ["Last 14 days"]);
        // Wraps correctly across a year boundary.
        assert_eq!(suggestions("december 2025", today()), ["December 2025"]);
    }

    #[test]
    fn every_suggestion_parses_back() {
        for label in suggestions("", today()) {
            let period = parse(&label, today()).unwrap_or_else(|| panic!("{label}"));
            assert_eq!(period.label(), label);
        }
    }

    #[test]
    fn parses_rolling_periods() {
        assert_eq!(parse("Last 24 hours", today()), Some(Period::LastDays(1)));
        assert_eq!(parse("24h", today()), Some(Period::LastDays(1)));
        assert_eq!(parse("last 7 days", today()), Some(Period::LastDays(7)));
        assert_eq!(parse(" 45d ", today()), Some(Period::LastDays(45)));
        assert_eq!(parse("3 days", today()), Some(Period::LastDays(3)));
        assert_eq!(parse("0d", today()), None);
        assert_eq!(parse("365d", today()), None);
    }

    #[test]
    fn parses_months() {
        assert_eq!(parse("July 2025", today()), month(2025, 7));
        assert_eq!(parse("jul 2025", today()), month(2025, 7));
        assert_eq!(parse("2025-07", today()), month(2025, 7));
        assert_eq!(parse("this month", today()), month(2026, 10));
        assert_eq!(parse("last month", today()), month(2026, 9));
        // A bare month is the most recent one.
        assert_eq!(parse("July", today()), month(2026, 7));
        assert_eq!(parse("October", today()), month(2026, 10));
        assert_eq!(parse("December", today()), month(2025, 12));
    }

    #[test]
    fn rejects_nonsense_and_the_future() {
        assert_eq!(parse("November 2026", today()), None);
        assert_eq!(parse("July 2027", today()), None);
        assert_eq!(parse("2026-13", today()), None);
        assert_eq!(parse("July 2025 please", today()), None);
        assert_eq!(parse("soon", today()), None);
        assert_eq!(parse("", today()), None);
    }

    #[test]
    fn month_bounds_follow_the_timezone() {
        let now = Utc.with_ymd_and_hms(2026, 10, 1, 12, 0, 0).unwrap();
        let paris: Tz = "Europe/Paris".parse().unwrap();

        // July in Paris is UTC+2: it starts and ends at 22:00 UTC.
        let (start, end) = Period::Month {
            year: 2026,
            month: 7,
        }
        .bounds(now, paris);
        assert_eq!(start, Utc.with_ymd_and_hms(2026, 6, 30, 22, 0, 0).unwrap());
        assert_eq!(end, Utc.with_ymd_and_hms(2026, 7, 31, 22, 0, 0).unwrap());

        // December rolls into the next year.
        let (start, end) = Period::Month {
            year: 2025,
            month: 12,
        }
        .bounds(now, chrono_tz::UTC);
        assert_eq!(start, Utc.with_ymd_and_hms(2025, 12, 1, 0, 0, 0).unwrap());
        assert_eq!(end, Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap());

        // The current month stops at now.
        let (_, end) = Period::Month {
            year: 2026,
            month: 10,
        }
        .bounds(now, chrono_tz::UTC);
        assert_eq!(end, now);

        let (start, end) = Period::LastDays(7).bounds(now, paris);
        assert_eq!((start, end), (now - Duration::days(7), now));
    }

    #[test]
    fn previous_bounds_end_where_the_period_starts() {
        let now = Utc.with_ymd_and_hms(2026, 10, 1, 12, 0, 0).unwrap();
        let paris: Tz = "Europe/Paris".parse().unwrap();

        for period in [
            Period::LastDays(14),
            Period::Month {
                year: 2026,
                month: 7,
            },
            Period::Month {
                year: 2026,
                month: 1,
            },
            Period::Month {
                year: 2026,
                month: 10,
            },
        ] {
            let (start, _) = period.bounds(now, paris);
            let (previous_start, previous_end) = period.previous_bounds(now, paris);
            assert_eq!(previous_end, start, "{period:?}");
            assert!(previous_start < previous_end, "{period:?}");
        }

        // The same length again for rolling periods, the whole month before
        // for a month (January reaches back into the previous year).
        let (start, _) = Period::LastDays(14).previous_bounds(now, paris);
        assert_eq!(start, now - Duration::days(28));
        let (start, _) = Period::Month {
            year: 2026,
            month: 1,
        }
        .previous_bounds(now, chrono_tz::UTC);
        assert_eq!(start, Utc.with_ymd_and_hms(2025, 12, 1, 0, 0, 0).unwrap());
    }
}
