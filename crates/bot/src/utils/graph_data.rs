use bonbon::prelude::{GraphEntry, GraphScaling, GraphTreatment, MiniGraph, OnBoard, SeriesPoint};
use chrono::{DateTime, Duration, Utc};

/// How far before the graph's start treatments are fetched, so doses and meals
/// given earlier still count toward what is on board when the graph begins.
/// Also the longest duration of insulin action we will take from a profile.
pub const ON_BOARD_LOOKBACK_HOURS: i64 = 6;

/// Shortest duration of insulin action we will take from a profile.
const MIN_DIA_HOURS: f64 = 2.0;

/// Duration of insulin action used when the profile has none.
const DEFAULT_DIA_HOURS: f64 = 4.0;

/// How long carbs take to absorb when working COB out from treatments.
const CARB_ABSORPTION_HOURS: i64 = 3;

/// Smallest reported IOB (units) and COB (grams) that count as something being
/// on board. Below these the value would read as zero on the graph.
const MIN_REPORTED_IOB: f32 = 0.05;
const MIN_REPORTED_COB: f32 = 0.5;

/// The glucose range a graph shows when the readings fit inside it, and the
/// widest it normally stretches to, in mg/dL.
const DEFAULT_Y_RANGE: (f32, f32) = (60.0, 200.0);
const Y_CLAMP: (f32, f32) = (40.0, 400.0);

/// Least clear space kept between the highest value drawn and the top of the
/// plot, in mg/dL, so the peak never touches the frame.
const PEAK_HEADROOM_MGDL: f32 = 20.0;

/// The vertical scale for a graph spanning `start` to `end`: the default
/// range, growing as the readings need, with the top always at least
/// [`PEAK_HEADROOM_MGDL`] above the highest reading or fingerprick shown.
pub fn y_scaling(
    entries: &[GraphEntry],
    treatments: &[GraphTreatment],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> GraphScaling {
    let shown = |date: DateTime<Utc>| date >= start && date <= end;
    let peak = entries
        .iter()
        .filter(|e| shown(e.date))
        .map(|e| e.sgv)
        .chain(
            treatments
                .iter()
                .filter(|t| shown(t.date))
                .filter_map(|t| t.mbg),
        )
        .filter(|v| v.is_finite())
        .fold(None, |peak: Option<f32>, v| {
            Some(peak.map_or(v, |p| p.max(v)))
        });

    // Rounded up to a ten so the top of the plot stays a round number.
    let top = peak.map_or(DEFAULT_Y_RANGE.1, |peak| {
        ((peak + PEAK_HEADROOM_MGDL) / 10.0).ceil() * 10.0
    });

    GraphScaling::Dynamic {
        clamp_min: Y_CLAMP.0,
        // A reading past the usual ceiling still gets its headroom.
        clamp_max: Y_CLAMP.1.max(top),
        default_min: DEFAULT_Y_RANGE.0,
        default_max: DEFAULT_Y_RANGE.1.max(top),
    }
}

/// The mini graphs worth drawing under a glucose graph spanning `start` to
/// `end`: IOB and/or COB, each only when there is something on board at some
/// point in the window.
///
/// Values an AID system reported are preferred. Without them the curve is
/// worked out from the treatments, each fading over the profile's duration of
/// insulin action (`dia_hours`) for insulin or a fixed absorption time for
/// carbs.
pub fn mini_graphs(
    reported_iob: Vec<SeriesPoint>,
    reported_cob: Vec<SeriesPoint>,
    treatments: &[GraphTreatment],
    dia_hours: Option<f64>,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Vec<MiniGraph> {
    let dia_hours = dia_hours
        .filter(|d| d.is_finite() && *d > 0.0)
        .unwrap_or(DEFAULT_DIA_HOURS)
        .clamp(MIN_DIA_HOURS, ON_BOARD_LOOKBACK_HOURS as f64);
    let dia = Duration::minutes((dia_hours * 60.0) as i64);
    let absorption = Duration::hours(CARB_ABSORPTION_HOURS);

    // Still on board during the window if given less than `duration` before it.
    let any_given = |amount: fn(&GraphTreatment) -> Option<f32>, duration: Duration| {
        treatments.iter().any(|t| {
            amount(t).is_some_and(|a| a > 0.0) && t.date > start - duration && t.date <= end
        })
    };
    let in_window = |points: &[SeriesPoint], min: f32| {
        points
            .iter()
            .any(|p| p.date >= start && p.date <= end && p.value.abs() >= min)
    };

    let mut graphs = Vec::new();

    // A single sample cannot be drawn as a curve.
    if reported_iob.len() >= 2 {
        if in_window(&reported_iob, MIN_REPORTED_IOB) {
            graphs.push(MiniGraph::iob_reported(reported_iob));
        }
    } else if any_given(|t| t.insulin, dia) {
        graphs.push(MiniGraph::iob(dia));
    }

    if reported_cob.len() >= 2 {
        if in_window(&reported_cob, MIN_REPORTED_COB) {
            graphs.push(MiniGraph::cob_reported(reported_cob));
        }
    } else if any_given(|t| t.carbs, absorption) {
        graphs.push(MiniGraph::cob(absorption));
    }

    graphs
}

/// Takes microboluses off a graph: insulin doses of `threshold` units or less
/// that come without carbs. A treatment that also carries a BG check keeps
/// that reading and only loses its insulin.
pub fn hide_microboluses(treatments: &[GraphTreatment], threshold: f32) -> Vec<GraphTreatment> {
    treatments
        .iter()
        .filter_map(|t| {
            let is_micro = t.carbs.is_none() && t.insulin.is_some_and(|units| units <= threshold);
            if !is_micro {
                return Some(t.clone());
            }
            t.mbg.is_some().then(|| GraphTreatment {
                insulin: None,
                ..t.clone()
            })
        })
        .collect()
}

/// Fixes an IOB mini graph that would be worked out from the graph's own
/// treatments to the curve `treatments` give instead, sampled every minute
/// from `start` to `end`.
///
/// Used when some doses are kept off the graph (hidden microboluses): the
/// graph only knows the treatments it draws, so left alone its IOB would
/// drop them too.
pub fn pin_iob(
    graphs: Vec<MiniGraph>,
    treatments: &[GraphTreatment],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Vec<MiniGraph> {
    graphs
        .into_iter()
        .map(|graph| match graph {
            MiniGraph::Iob(OnBoard::FromTreatments(duration)) => {
                let doses: Vec<(DateTime<Utc>, f32)> = treatments
                    .iter()
                    .filter_map(|t| t.insulin.map(|units| (t.date, units)))
                    .collect();
                MiniGraph::iob_reported(on_board_series(&doses, duration, start, end))
            }
            other => other,
        })
        .collect()
}

/// What is on board each minute from `start` to `end`, with every dose
/// counting in full when given and fading in a straight line to nothing over
/// `duration` (the same model the graph uses for its own treatments).
fn on_board_series(
    doses: &[(DateTime<Utc>, f32)],
    duration: Duration,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Vec<SeriesPoint> {
    let span = duration.num_seconds() as f32;
    let minutes = (end - start).num_minutes().max(0);
    (0..=minutes)
        .map(|m| {
            let date = start + Duration::minutes(m);
            let value = doses
                .iter()
                .map(|&(given, amount)| {
                    let age = (date - given).num_seconds() as f32;
                    if span > 0.0 && (0.0..span).contains(&age) {
                        amount * (1.0 - age / span)
                    } else {
                        0.0
                    }
                })
                .sum();
            SeriesPoint { value, date }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(minutes: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000, 0).unwrap() + Duration::minutes(minutes)
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

    fn series(values: &[f32]) -> Vec<SeriesPoint> {
        values
            .iter()
            .enumerate()
            .map(|(i, &value)| SeriesPoint {
                value,
                date: at(i as i64 * 5),
            })
            .collect()
    }

    fn kinds(graphs: &[MiniGraph]) -> Vec<&'static str> {
        graphs
            .iter()
            .map(|g| match g {
                MiniGraph::Iob(OnBoard::Reported(_)) => "iob reported",
                MiniGraph::Iob(OnBoard::FromTreatments(_)) => "iob treatments",
                MiniGraph::Cob(OnBoard::Reported(_)) => "cob reported",
                MiniGraph::Cob(OnBoard::FromTreatments(_)) => "cob treatments",
                _ => "other",
            })
            .collect()
    }

    fn top_of(scaling: GraphScaling) -> (f32, f32) {
        match scaling {
            GraphScaling::Dynamic {
                clamp_max,
                default_max,
                ..
            } => (default_max, clamp_max),
            GraphScaling::Static { max, .. } => (max, max),
        }
    }

    fn readings(values: &[(i64, f32)]) -> Vec<GraphEntry> {
        values
            .iter()
            .map(|&(minutes, sgv)| GraphEntry {
                sgv,
                date: at(minutes),
            })
            .collect()
    }

    #[test]
    fn y_scaling_keeps_headroom_above_the_peak() {
        let (start, end) = (at(0), at(180));

        // Well inside the default range: nothing changes.
        let calm = readings(&[(10, 110.0), (60, 150.0)]);
        assert_eq!(top_of(y_scaling(&calm, &[], start, end)), (200.0, 400.0));

        // A peak of 190 would sit 10 below the default top: make room.
        let near = readings(&[(10, 110.0), (60, 190.0)]);
        assert_eq!(top_of(y_scaling(&near, &[], start, end)), (210.0, 400.0));

        // A peak above the default top, rounded up to the next ten.
        let high = readings(&[(10, 110.0), (60, 263.0)]);
        assert_eq!(top_of(y_scaling(&high, &[], start, end)), (290.0, 400.0));

        // Even past the usual ceiling.
        let extreme = readings(&[(60, 395.0)]);
        assert_eq!(top_of(y_scaling(&extreme, &[], start, end)), (420.0, 420.0));
    }

    #[test]
    fn y_scaling_only_counts_what_is_shown() {
        let (start, end) = (at(0), at(180));

        // A high reading before the window does not stretch the graph.
        let earlier = readings(&[(-10, 300.0), (60, 120.0)]);
        assert_eq!(top_of(y_scaling(&earlier, &[], start, end)), (200.0, 400.0));

        // A fingerprick in the window does.
        let fingerprick = GraphTreatment {
            mbg: Some(245.0),
            ..treatment(90, None, None)
        };
        let calm = readings(&[(60, 120.0)]);
        assert_eq!(
            top_of(y_scaling(&calm, &[fingerprick], start, end)),
            (270.0, 400.0)
        );

        // No data: the default range.
        assert_eq!(top_of(y_scaling(&[], &[], start, end)), (200.0, 400.0));
    }

    #[test]
    fn mini_graphs_only_when_something_is_on_board() {
        let (start, end) = (at(0), at(180));

        // Nothing at all.
        assert!(mini_graphs(vec![], vec![], &[], None, start, end).is_empty());

        // Reported values that never leave zero.
        let graphs = mini_graphs(
            series(&[0.0, 0.01, 0.0]),
            series(&[0.0, 0.0, 0.0]),
            &[],
            None,
            start,
            end,
        );
        assert!(graphs.is_empty());

        // Reported IOB only.
        let graphs = mini_graphs(
            series(&[1.2, 1.0, 0.8]),
            series(&[0.0, 0.0, 0.0]),
            &[],
            None,
            start,
            end,
        );
        assert_eq!(kinds(&graphs), vec!["iob reported"]);

        // Reported COB only.
        let graphs = mini_graphs(vec![], series(&[30.0, 25.0]), &[], None, start, end);
        assert_eq!(kinds(&graphs), vec!["cob reported"]);
    }

    #[test]
    fn mini_graphs_fall_back_to_treatments() {
        let (start, end) = (at(0), at(180));

        let both = [treatment(30, Some(4.0), Some(45.0))];
        let graphs = mini_graphs(vec![], vec![], &both, Some(5.0), start, end);
        assert_eq!(kinds(&graphs), vec!["iob treatments", "cob treatments"]);

        let carbs_only = [treatment(30, None, Some(15.0))];
        let graphs = mini_graphs(vec![], vec![], &carbs_only, None, start, end);
        assert_eq!(kinds(&graphs), vec!["cob treatments"]);

        // A bolus given before the window still counts while it is active
        // (default 4 h), but one that has fully worn off does not.
        let earlier = [treatment(-120, Some(3.0), None)];
        let graphs = mini_graphs(vec![], vec![], &earlier, None, start, end);
        assert_eq!(kinds(&graphs), vec!["iob treatments"]);

        let worn_off = [treatment(-300, Some(3.0), Some(40.0))];
        assert!(mini_graphs(vec![], vec![], &worn_off, None, start, end).is_empty());
    }

    #[test]
    fn hides_only_carbless_small_doses() {
        let treatments = [
            treatment(0, Some(0.3), None),
            treatment(5, Some(0.5), None),
            treatment(10, Some(0.6), None),
            // A small dose given with carbs is a meal bolus, not a microbolus.
            treatment(15, Some(0.4), Some(10.0)),
            treatment(20, None, Some(20.0)),
            GraphTreatment {
                mbg: Some(110.0),
                ..treatment(25, Some(0.2), None)
            },
        ];

        let shown = hide_microboluses(&treatments, 0.5);
        let summary: Vec<_> = shown.iter().map(|t| (t.insulin, t.carbs, t.mbg)).collect();
        assert_eq!(
            summary,
            vec![
                (Some(0.6), None, None),
                (Some(0.4), Some(10.0), None),
                (None, Some(20.0), None),
                (None, None, Some(110.0)),
            ]
        );
    }

    #[test]
    fn pinned_iob_keeps_every_dose() {
        let (start, end) = (at(0), at(240));
        // A bolus before the window and a microbolus inside it.
        let treatments = [
            treatment(-60, Some(4.0), None),
            treatment(60, Some(0.4), None),
        ];

        let graphs = vec![
            MiniGraph::iob(Duration::hours(4)),
            MiniGraph::cob(Duration::hours(3)),
        ];
        let pinned = pin_iob(graphs, &treatments, start, end);
        assert_eq!(kinds(&pinned), vec!["iob reported", "cob treatments"]);

        let MiniGraph::Iob(OnBoard::Reported(points)) = &pinned[0] else {
            panic!("IOB should be pinned");
        };
        let value_at = |minutes: i64| {
            points
                .iter()
                .find(|p| p.date == at(minutes))
                .map(|p| p.value)
                .unwrap()
        };
        assert_eq!(points.len(), 241);
        // 4 U an hour into a 4 h fade.
        assert!((value_at(0) - 3.0).abs() < 1e-4);
        // The microbolus counts in full the minute it is given.
        assert!((value_at(60) - (2.0 + 0.4)).abs() < 1e-4);
        // The bolus has worn off; the microbolus is two hours in.
        assert!((value_at(180) - 0.2).abs() < 1e-4);
    }
}
