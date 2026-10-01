use bonbon::prelude::{GraphTreatment, MiniGraph, OnBoard, SeriesPoint};
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
