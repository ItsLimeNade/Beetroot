use bonbon::prelude::GraphTreatment;
use chrono::{DateTime, Duration, Utc};

/// Readings further apart than this are not treated as one continuous trace:
/// an episode never spans a gap in the data.
const MAX_GAP_MINUTES: i64 = 15;

/// Shortest glucose episode worth reporting. Also how long glucose has to stay
/// out of an episode's band before the episode counts as over, so a single
/// stray reading neither creates an episode nor splits one in two.
pub const MIN_EPISODE_MINUTES: i64 = 15;

/// Graph window around a result: at least this much context before and after
/// an episode, within these overall bounds.
const EPISODE_PADDING_HOURS: i64 = 1;
const MIN_WINDOW_HOURS: i64 = 3;
const MAX_WINDOW_HOURS: i64 = 24;

/// Graph window around a treatment: a little of what led up to it and more of
/// what followed.
const TREATMENT_WINDOW_HOURS: i64 = 6;
const TREATMENT_LEAD_HOURS: i64 = 2;

/// What a search looks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Lows,
    Highs,
    InRange,
    Carbs,
    Insulin,
}

impl Kind {
    /// Whether results are stretches of glucose readings, as opposed to
    /// single treatments.
    pub fn is_episode(self) -> bool {
        matches!(self, Self::Lows | Self::Highs | Self::InRange)
    }

    /// The orders that make sense for this kind, default first.
    pub fn sorts(self) -> &'static [Sort] {
        match self {
            Self::Lows => &[
                Sort::Recent,
                Sort::Lowest,
                Sort::Longest,
                Sort::Shortest,
                Sort::Oldest,
            ],
            Self::Highs => &[
                Sort::Recent,
                Sort::Highest,
                Sort::Longest,
                Sort::Shortest,
                Sort::Oldest,
            ],
            Self::InRange => &[Sort::Recent, Sort::Longest, Sort::Shortest, Sort::Oldest],
            Self::Carbs | Self::Insulin => {
                &[Sort::Recent, Sort::Highest, Sort::Lowest, Sort::Oldest]
            }
        }
    }
}

/// How results are ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Recent,
    Oldest,
    Longest,
    Shortest,
    /// By [`Hit::value`], largest first.
    Highest,
    /// By [`Hit::value`], smallest first.
    Lowest,
}

/// One glucose reading, in mg/dL.
#[derive(Debug, Clone, Copy)]
pub struct Reading {
    pub date: DateTime<Utc>,
    pub sgv: f32,
}

/// Which reading of an episode stands for it.
#[derive(Debug, Clone, Copy)]
pub enum Extreme {
    Min,
    Max,
    Mean,
}

/// One search result: a glucose episode or a treatment.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub start: DateTime<Utc>,
    /// Same as `start` for a treatment.
    pub end: DateTime<Utc>,
    /// The moment a graph of this result should be built around: the lowest
    /// or highest reading of an episode, or the treatment itself.
    pub focus: DateTime<Utc>,
    /// The number this result is known by: an episode's lowest, highest or
    /// average reading in mg/dL, a treatment's grams of carbs or units of
    /// insulin.
    pub value: f32,
    /// The other half of a treatment that carries both carbs and insulin.
    pub companion: Option<f32>,
    /// The episode was still going at the latest reading.
    pub ongoing: bool,
}

impl Hit {
    pub fn duration(&self) -> Duration {
        self.end - self.start
    }
}

/// A stretch of consecutive matching readings, by index.
struct Run {
    first: usize,
    last: usize,
    end: DateTime<Utc>,
    /// Ended because glucose left the band, rather than at a gap in the data.
    recovered: bool,
    ongoing: bool,
}

/// Finds every episode where glucose stayed in a band (`matches`) for at
/// least [`MIN_EPISODE_MINUTES`].
///
/// `readings` must be sorted oldest first. An episode starts at its first
/// matching reading and ends at the first reading back outside the band; it
/// survives excursions out of the band shorter than [`MIN_EPISODE_MINUTES`],
/// but never spans a gap in the data.
pub fn find_episodes(
    readings: &[Reading],
    matches: impl Fn(f32) -> bool,
    extreme: Extreme,
    now: DateTime<Utc>,
) -> Vec<Hit> {
    let max_gap = Duration::minutes(MAX_GAP_MINUTES);
    let min_episode = Duration::minutes(MIN_EPISODE_MINUTES);

    let mut runs: Vec<Run> = Vec::new();
    let mut open: Option<(usize, usize)> = None;
    for (i, reading) in readings.iter().enumerate() {
        let contiguous = i > 0 && reading.date - readings[i - 1].date <= max_gap;
        let matching = matches(reading.sgv);

        if let Some((first, last)) = open {
            if contiguous && matching {
                open = Some((first, i));
                continue;
            }
            runs.push(Run {
                first,
                last,
                end: if contiguous {
                    reading.date
                } else {
                    readings[last].date
                },
                recovered: contiguous,
                ongoing: false,
            });
            open = None;
        }
        if matching {
            open = Some((i, i));
        }
    }
    if let Some((first, last)) = open {
        let end = readings[last].date;
        runs.push(Run {
            first,
            last,
            end,
            recovered: false,
            ongoing: now - end <= max_gap,
        });
    }

    // Join runs separated only by a brief excursion out of the band.
    let mut merged: Vec<Run> = Vec::new();
    for run in runs {
        match merged.last_mut() {
            Some(prev)
                if prev.recovered
                    && readings[run.first].date - prev.end < min_episode
                    && readings[prev.last..=run.first]
                        .windows(2)
                        .all(|w| w[1].date - w[0].date <= max_gap) =>
            {
                prev.last = run.last;
                prev.end = run.end;
                prev.recovered = run.recovered;
                prev.ongoing = run.ongoing;
            }
            _ => merged.push(run),
        }
    }

    merged
        .into_iter()
        .filter_map(|run| {
            let start = readings[run.first].date;
            if run.end - start < min_episode {
                return None;
            }
            let span = &readings[run.first..=run.last];
            let (value, focus) = match extreme {
                Extreme::Min => span
                    .iter()
                    .min_by(|a, b| a.sgv.total_cmp(&b.sgv))
                    .map(|r| (r.sgv, r.date))?,
                Extreme::Max => span
                    .iter()
                    .max_by(|a, b| a.sgv.total_cmp(&b.sgv))
                    .map(|r| (r.sgv, r.date))?,
                Extreme::Mean => (
                    span.iter().map(|r| r.sgv).sum::<f32>() / span.len() as f32,
                    start + (run.end - start) / 2,
                ),
            };
            Some(Hit {
                start,
                end: run.end,
                focus,
                value,
                companion: None,
                ongoing: run.ongoing,
            })
        })
        .collect()
}

/// Turns treatments into results for a [`Kind::Carbs`] or [`Kind::Insulin`]
/// search, keeping those whose amount passes `keep`.
pub fn treatment_hits(
    treatments: &[GraphTreatment],
    kind: Kind,
    keep: impl Fn(f32) -> bool,
) -> Vec<Hit> {
    treatments
        .iter()
        .filter_map(|t| {
            let (value, companion) = match kind {
                Kind::Carbs => (t.carbs?, t.insulin),
                Kind::Insulin => (t.insulin?, t.carbs),
                _ => return None,
            };
            (value > 0.0 && keep(value)).then_some(Hit {
                start: t.date,
                end: t.date,
                focus: t.date,
                value,
                companion: companion.filter(|c| *c > 0.0),
                ongoing: false,
            })
        })
        .collect()
}

/// Orders results, most recent first among equals.
pub fn sort_hits(hits: &mut [Hit], sort: Sort) {
    hits.sort_by(|a, b| {
        let primary = match sort {
            Sort::Recent => std::cmp::Ordering::Equal,
            Sort::Oldest => a.start.cmp(&b.start),
            Sort::Longest => b.duration().cmp(&a.duration()),
            Sort::Shortest => a.duration().cmp(&b.duration()),
            Sort::Highest => b.value.total_cmp(&a.value),
            Sort::Lowest => a.value.total_cmp(&b.value),
        };
        primary.then_with(|| b.start.cmp(&a.start))
    });
}

/// The window a graph of `hit` should show, as its start and length.
///
/// An episode is centered with some context either side. One too long to fit
/// is shown around its lowest or highest point instead. A treatment sits a
/// third of the way in, since what follows it matters most. The start snaps to
/// a quarter hour, and the window never reaches past `now`.
pub fn graph_window(hit: &Hit, kind: Kind, now: DateTime<Utc>) -> (DateTime<Utc>, Duration) {
    let (start, length) = if kind.is_episode() {
        let padded = hit.duration() + Duration::hours(2 * EPISODE_PADDING_HOURS);
        // Whole hours keep the time axis tidy.
        let hours = (padded.num_minutes() + 59) / 60;
        if hours <= MAX_WINDOW_HOURS {
            let length = Duration::hours(hours.max(MIN_WINDOW_HOURS));
            let middle = hit.start + hit.duration() / 2;
            (middle - length / 2, length)
        } else {
            let length = Duration::hours(MAX_WINDOW_HOURS);
            (hit.focus - length / 2, length)
        }
    } else {
        (
            hit.start - Duration::hours(TREATMENT_LEAD_HOURS),
            Duration::hours(TREATMENT_WINDOW_HOURS),
        )
    };

    (round_to_quarter_hour(start).min(now - length), length)
}

/// Snaps a graph's start to the nearest quarter hour, for tidy axis labels.
fn round_to_quarter_hour(date: DateTime<Utc>) -> DateTime<Utc> {
    const QUARTER: i64 = 15 * 60;
    let rounded = (date.timestamp() + QUARTER / 2).div_euclid(QUARTER) * QUARTER;
    DateTime::from_timestamp(rounded, 0).unwrap_or(date)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(minutes: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000, 0).unwrap() + Duration::minutes(minutes)
    }

    /// Readings five minutes apart, starting at minute zero.
    fn trace(values: &[f32]) -> Vec<Reading> {
        values
            .iter()
            .enumerate()
            .map(|(i, &sgv)| Reading {
                date: at(i as i64 * 5),
                sgv,
            })
            .collect()
    }

    fn lows(readings: &[Reading]) -> Vec<Hit> {
        find_episodes(readings, |v| v < 70.0, Extreme::Min, at(100_000))
    }

    fn treatment(minutes: i64, insulin: Option<f32>, carbs: Option<f32>) -> GraphTreatment {
        GraphTreatment {
            insulin,
            carbs,
            mbg: None,
            date: at(minutes),
            is_isf: false,
        }
    }

    fn hit(start: i64, minutes: i64, value: f32) -> Hit {
        Hit {
            start: at(start),
            end: at(start + minutes),
            focus: at(start),
            value,
            companion: None,
            ongoing: false,
        }
    }

    #[test]
    fn finds_an_episode_with_its_bounds_and_extreme() {
        let readings = trace(&[100.0, 68.0, 60.0, 52.0, 64.0, 90.0, 100.0]);
        let found = lows(&readings);

        assert_eq!(found.len(), 1);
        let low = &found[0];
        // From the first low reading to the first one back in range.
        assert_eq!((low.start, low.end), (at(5), at(25)));
        assert_eq!(low.duration(), Duration::minutes(20));
        assert_eq!((low.value, low.focus), (52.0, at(15)));
        assert!(!low.ongoing);
    }

    #[test]
    fn ignores_blips_shorter_than_the_minimum() {
        // Two low readings: ten minutes from first low to recovery.
        let readings = trace(&[100.0, 65.0, 66.0, 90.0, 100.0]);
        assert!(lows(&readings).is_empty());

        // Three low readings: fifteen minutes.
        let readings = trace(&[100.0, 65.0, 66.0, 67.0, 90.0]);
        assert_eq!(lows(&readings).len(), 1);
    }

    #[test]
    fn brief_recovery_does_not_split_an_episode() {
        // Back in range for a single reading (5 min), then low again.
        let readings = trace(&[100.0, 65.0, 60.0, 72.0, 64.0, 58.0, 66.0, 95.0]);
        let found = lows(&readings);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].start, found[0].end), (at(5), at(35)));
        assert_eq!(found[0].value, 58.0);

        // Back in range for three readings: fifteen minutes from recovery to
        // the next low, so these are two separate lows.
        let readings = trace(&[65.0, 60.0, 62.0, 80.0, 85.0, 82.0, 64.0, 58.0, 61.0, 90.0]);
        assert_eq!(lows(&readings).len(), 2);
    }

    #[test]
    fn never_spans_a_gap_in_the_data() {
        let mut readings = trace(&[65.0, 60.0, 62.0, 61.0]);
        // Sensor back an hour later, still low.
        readings.extend([75, 80, 85, 90].map(|m| Reading {
            date: at(m),
            sgv: 63.0,
        }));
        readings.push(Reading {
            date: at(95),
            sgv: 100.0,
        });

        let found = lows(&readings);
        assert_eq!(found.len(), 2);
        // Cut off by the gap: ends at its last reading.
        assert_eq!((found[0].start, found[0].end), (at(0), at(15)));
        assert_eq!((found[1].start, found[1].end), (at(75), at(95)));
    }

    #[test]
    fn flags_an_episode_still_going() {
        let readings = trace(&[100.0, 65.0, 60.0, 58.0, 55.0]);
        let last = readings.last().unwrap().date;

        let found = find_episodes(&readings, |v| v < 70.0, Extreme::Min, last);
        assert!(found[0].ongoing);
        assert_eq!(found[0].end, last);

        // Old data that simply stops is not "ongoing".
        let found = find_episodes(
            &readings,
            |v| v < 70.0,
            Extreme::Min,
            last + Duration::hours(3),
        );
        assert!(!found[0].ongoing);
    }

    #[test]
    fn in_range_episodes_report_their_average() {
        let readings = trace(&[60.0, 100.0, 120.0, 140.0, 200.0]);
        let found = find_episodes(
            &readings,
            |v| (70.0..=180.0).contains(&v),
            Extreme::Mean,
            at(100_000),
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].value, 120.0);
        assert_eq!((found[0].start, found[0].end), (at(5), at(20)));
    }

    #[test]
    fn treatment_hits_pick_the_right_amount() {
        let treatments = [
            treatment(0, Some(4.0), Some(45.0)),
            treatment(30, Some(0.2), None),
            treatment(60, None, Some(15.0)),
            treatment(90, Some(0.0), Some(0.0)),
        ];

        let carbs = treatment_hits(&treatments, Kind::Carbs, |_| true);
        assert_eq!(
            carbs.iter().map(|h| h.value).collect::<Vec<_>>(),
            vec![45.0, 15.0]
        );
        assert_eq!(carbs[0].companion, Some(4.0));
        assert_eq!(carbs[1].companion, None);

        let insulin = treatment_hits(&treatments, Kind::Insulin, |units| units > 0.5);
        assert_eq!(insulin.len(), 1);
        assert_eq!((insulin[0].value, insulin[0].companion), (4.0, Some(45.0)));
    }

    #[test]
    fn sorts_every_way() {
        let mut hits = vec![hit(0, 30, 60.0), hit(100, 90, 50.0), hit(300, 20, 65.0)];
        let order = |hits: &[Hit]| hits.iter().map(|h| h.value).collect::<Vec<_>>();

        sort_hits(&mut hits, Sort::Recent);
        assert_eq!(order(&hits), vec![65.0, 50.0, 60.0]);
        sort_hits(&mut hits, Sort::Oldest);
        assert_eq!(order(&hits), vec![60.0, 50.0, 65.0]);
        sort_hits(&mut hits, Sort::Longest);
        assert_eq!(order(&hits), vec![50.0, 60.0, 65.0]);
        sort_hits(&mut hits, Sort::Shortest);
        assert_eq!(order(&hits), vec![65.0, 60.0, 50.0]);
        sort_hits(&mut hits, Sort::Lowest);
        assert_eq!(order(&hits), vec![50.0, 60.0, 65.0]);
        sort_hits(&mut hits, Sort::Highest);
        assert_eq!(order(&hits), vec![65.0, 60.0, 50.0]);

        // Equal values: the most recent comes first.
        let mut ties = vec![hit(0, 30, 55.0), hit(500, 30, 55.0)];
        sort_hits(&mut ties, Sort::Lowest);
        assert_eq!(ties[0].start, at(500));
    }

    #[test]
    fn graph_window_centers_an_episode() {
        let now = at(100_000);

        // A 40 min low: padded to the 3 h minimum, centered on its middle
        // (minute 620) to the nearest quarter hour.
        let (start, length) = graph_window(&hit(600, 40, 55.0), Kind::Lows, now);
        assert_eq!(length, Duration::hours(3));
        assert_eq!(start, at(525));

        // A 5 h high: an hour of context either side.
        let (start, length) = graph_window(&hit(600, 300, 250.0), Kind::Highs, now);
        assert_eq!(length, Duration::hours(7));
        assert_eq!(start, at(540));

        // Too long to fit: a day around its extreme point.
        let mut long = hit(600, 3 * 24 * 60, 110.0);
        long.focus = at(2000);
        let (start, length) = graph_window(&long, Kind::InRange, now);
        assert_eq!(length, Duration::hours(24));
        assert_eq!(start, at(1275));
    }

    #[test]
    fn graph_window_follows_a_treatment_and_stops_at_now() {
        let (start, length) = graph_window(&hit(600, 0, 45.0), Kind::Carbs, at(100_000));
        assert_eq!(length, Duration::hours(6));
        assert_eq!(start, at(480));

        // A meal half an hour ago: the window ends now instead of running
        // into the future.
        let now = at(630);
        let (start, length) = graph_window(&hit(600, 0, 45.0), Kind::Carbs, now);
        assert_eq!(start + length, now);
    }
}
