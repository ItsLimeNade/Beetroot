use bonbon::prelude::{GraphEntry, GraphTreatment, MiniGraph, SeriesPoint};
use chrono::{DateTime, Duration, Utc};
use cinnamon::models::devicestatus::DeviceStatus;
use cinnamon::models::entries::SgvEntry;
use cinnamon::models::treatments::Treatment;
use serde_json::Value;

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

pub fn graph_entry(entry: SgvEntry) -> GraphEntry {
    GraphEntry {
        sgv: entry.sgv as f32,
        date: DateTime::from_timestamp_millis(entry.date).unwrap_or_else(Utc::now),
    }
}

/// `None` when the treatment has no readable timestamp, so it cannot be placed
/// on a graph.
pub fn graph_treatment(t: Treatment) -> Option<GraphTreatment> {
    let date = DateTime::parse_from_rfc3339(&t.created_at)
        .ok()?
        .with_timezone(&Utc);
    // `glucose` is stored in the treatment's own units.
    let is_mmol = t
        .units
        .as_deref()
        .is_some_and(|u| u.trim().to_ascii_lowercase().starts_with("mmol"));

    Some(GraphTreatment {
        insulin: t.insulin.map(|v| v as f32),
        carbs: t.carbs.map(|v| v as f32),
        mbg: t.glucose.map(|v| if is_mmol { v * 18.0 } else { v } as f32),
        date,
        is_isf: false,
    })
}

/// Splits Nightscout device statuses into the insulin on board and carbs on
/// board an AID system reported, as `(iob, cob)`.
///
/// Reads the `openaps` block (AAPS, Trio, OpenAPS) and the `loop` block
/// (Loop). Statuses without a readable date or without a value are skipped.
pub fn reported_on_board(statuses: &[DeviceStatus]) -> (Vec<SeriesPoint>, Vec<SeriesPoint>) {
    let mut iob = Vec::new();
    let mut cob = Vec::new();
    for status in statuses {
        let Some(date) = status_time(status) else {
            continue;
        };
        let openaps = status.openaps.as_ref();
        let suggested = openaps.and_then(|o| o.get("suggested"));
        let enacted = openaps.and_then(|o| o.get("enacted"));
        // cinnamon's `loop_` field is not renamed, so the `loop` block lands
        // in the flattened extras.
        let loop_ = status.extra.get("loop").or(status.loop_.as_ref());

        // oref0 uploads `iob` as a list of forecasts, the current one first.
        let iob_value = openaps
            .and_then(|o| o.get("iob"))
            .and_then(|iob| match iob {
                Value::Array(forecasts) => forecasts.first(),
                other => Some(other),
            })
            .and_then(|iob| number(iob, "iob"))
            .or_else(|| suggested.and_then(|s| number(s, "IOB")))
            .or_else(|| {
                loop_
                    .and_then(|l| l.get("iob"))
                    .and_then(|v| number(v, "iob"))
            });
        let cob_value = suggested
            .and_then(|s| number(s, "COB"))
            .or_else(|| enacted.and_then(|e| number(e, "COB")))
            .or_else(|| {
                loop_
                    .and_then(|l| l.get("cob"))
                    .and_then(|v| number(v, "cob"))
            });

        if let Some(value) = iob_value {
            iob.push(SeriesPoint {
                value: value as f32,
                date,
            });
        }
        if let Some(value) = cob_value {
            cob.push(SeriesPoint {
                value: value as f32,
                date,
            });
        }
    }
    (iob, cob)
}

fn number(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(Value::as_f64)
}

fn status_time(status: &DeviceStatus) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&status.created_at)
        .ok()
        .map(|d| d.with_timezone(&Utc))
        .or_else(|| {
            status
                .extra
                .get("mills")
                .and_then(Value::as_i64)
                .and_then(DateTime::from_timestamp_millis)
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use bonbon::prelude::OnBoard;
    use serde_json::json;

    fn at(minutes: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000, 0).unwrap() + Duration::minutes(minutes)
    }

    fn status(value: Value) -> DeviceStatus {
        serde_json::from_value(value).unwrap()
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
    fn reads_reported_on_board_from_every_uploader() {
        let statuses = [
            // AAPS / Trio: `iob` is an object, COB sits in `suggested`.
            status(json!({
                "created_at": "2026-01-01T10:00:00Z",
                "openaps": { "iob": { "iob": 1.25 }, "suggested": { "COB": 20 } }
            })),
            // oref0: `iob` is a list of forecasts, COB only in `enacted`.
            status(json!({
                "created_at": "2026-01-01T10:05:00Z",
                "openaps": { "iob": [{ "iob": -0.4 }, { "iob": -0.3 }], "enacted": { "COB": 0 } }
            })),
            // Loop.
            status(json!({
                "created_at": "2026-01-01T10:10:00Z",
                "loop": { "iob": { "iob": 2.5 }, "cob": { "cob": 12.0 } }
            })),
            // Unreadable date, pump-only status: both skipped.
            status(json!({ "created_at": "yesterday", "loop": { "iob": { "iob": 9.0 } } })),
            status(json!({ "created_at": "2026-01-01T10:15:00Z", "pump": { "reservoir": 80 } })),
        ];

        let (iob, cob) = reported_on_board(&statuses);
        let values = |points: &[SeriesPoint]| points.iter().map(|p| p.value).collect::<Vec<_>>();

        assert_eq!(values(&iob), vec![1.25, -0.4, 2.5]);
        assert_eq!(values(&cob), vec![20.0, 0.0, 12.0]);
        assert_eq!(iob[2].date.to_rfc3339(), "2026-01-01T10:10:00+00:00");
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
    fn converts_treatment_glucose_to_mgdl() {
        let t: Treatment = serde_json::from_value(json!({
            "eventType": "BG Check",
            "created_at": "2026-01-01T10:00:00Z",
            "glucose": 5.5,
            "units": "mmol"
        }))
        .unwrap();
        let converted = graph_treatment(t).unwrap();
        assert_eq!(converted.mbg, Some(99.0));
    }
}
