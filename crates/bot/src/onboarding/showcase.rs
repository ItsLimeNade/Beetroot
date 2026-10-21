//! The images shown during onboarding: real Beetroot renders of made-up data,
//! so people see what they get before connecting anything.

use crate::utils::render;
use anyhow::anyhow;
use bonbon::prelude::*;
use chrono::{DateTime, Duration, Utc};
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder, RgbaImage};
use std::sync::OnceLock;

/// The low and high targets the sample images are drawn with.
const TARGETS: (f32, f32) = (70.0, 180.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Showcase {
    /// A `/graph` of a sample day, with meals, insulin and on-board curves.
    Graph,
    /// The `/bg` card for the end of that day.
    Card,
    /// The `/tir` card for a sample week.
    TimeInRange,
    /// The same `/bg` card in a light theme.
    ThemedCard,
}

/// Each image is drawn once, the first time someone needs it.
static RENDERED: [OnceLock<Vec<u8>>; 4] = [const { OnceLock::new() }; 4];

impl Showcase {
    /// The attachment name, also used as `attachment://` in embeds.
    pub fn filename(self) -> &'static str {
        match self {
            Self::Graph => "beetroot_graph.png",
            Self::Card => "beetroot_card.png",
            Self::TimeInRange => "beetroot_tir.png",
            Self::ThemedCard => "beetroot_theme.png",
        }
    }

    /// The image as a PNG. `None` if it could not be drawn, in which case
    /// the page is shown without it.
    pub async fn png(self) -> Option<Vec<u8>> {
        let slot = &RENDERED[self as usize];
        if let Some(png) = slot.get() {
            return Some(png.clone());
        }
        match render::run_blocking(move || encode(&self.draw()?)).await {
            Ok(png) => Some(slot.get_or_init(|| png).clone()),
            Err(e) => {
                tracing::warn!(error = %e, image = self.filename(), "could not draw onboarding image");
                None
            }
        }
    }

    fn draw(self) -> anyhow::Result<RgbaImage> {
        let units = UnitDisplay::Dual {
            primary: UnitPreference::MgDl,
        };
        let (start, end) = sample_day();
        let image = match self {
            Self::Graph => {
                let (entries, treatments) = simulate_day(start, end);
                GlucoseGraphBuilder::new()
                    .with_layout(LayoutConfig {
                        width: 1275,
                        height: 825,
                        ..Default::default()
                    })
                    .with_theme(Theme::dark())
                    .with_font(crate::utils::font::INTER)
                    .with_units(units)
                    .with_targets(TARGETS.0, TARGETS.1)
                    .with_timezone(chrono_tz::UTC)
                    .with_microbolus_threshold(0.5)
                    .with_trace(false)
                    .with_entries(entries)
                    .with_treatments(treatments)
                    .with_time_axis(TimeAxisMode::EquallyDistributed { count: 6 })
                    .add_mini_graph(MiniGraph::iob(Duration::hours(4)))
                    .add_mini_graph(MiniGraph::cob(Duration::hours(3)))
                    .build()
            }
            Self::Card | Self::ThemedCard => {
                let theme = if self == Self::Card {
                    Theme::dark()
                } else {
                    Theme::ube_light()
                };
                BgCardBuilder::new()
                    .with_data(sample_card(start, end))
                    .with_targets(TARGETS.0, TARGETS.1)
                    .with_units(units)
                    .with_theme(theme)
                    .with_font(crate::utils::font::INTER)
                    .with_scale(2.0)
                    .build()
            }
            Self::TimeInRange => TimeInRangeBuilder::new()
                .with_entries(sample_week(end))
                .with_targets(TARGETS.0, TARGETS.1)
                .with_units(units)
                .with_period_label("Last 7 days")
                .with_extremes(true)
                .with_theme(Theme::dark())
                .with_font(crate::utils::font::INTER)
                .with_scale(2.0)
                .build(),
        };
        image.map_err(|e| anyhow!(e.to_string()))
    }
}

fn encode(image: &RgbaImage) -> anyhow::Result<Vec<u8>> {
    let mut png = Vec::with_capacity(200_000);
    PngEncoder::new_with_quality(&mut png, CompressionType::Level(6), FilterType::NoFilter)
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            ExtendedColorType::Rgba8,
        )?;
    Ok(png)
}

/// The sample day: 07:00 to 19:00 UTC, so the graph's clock reads like a real
/// day. It is the latest such day already over, since insulin and carbs on
/// board are only drawn up to now.
fn sample_day() -> (DateTime<Utc>, DateTime<Utc>) {
    let now = Utc::now();
    let mut end = now
        .date_naive()
        .and_hms_opt(19, 0, 0)
        .expect("19:00 is a valid time")
        .and_utc();
    if end > now {
        end -= Duration::days(1);
    }
    (end - Duration::hours(12), end)
}

/// Rise-then-fall shape peaking `peak` hours after `t = 0`.
fn bump(t: f32, peak: f32) -> f32 {
    if t <= 0.0 {
        0.0
    } else {
        (t / peak) * (1.0 - t / peak).exp()
    }
}

/// A deterministic day: three meals with their boluses, a few automatic
/// microboluses, a post-lunch high, and an afternoon low treated with juice.
fn simulate_day(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> (Vec<GraphEntry>, Vec<GraphTreatment>) {
    // (hours after start, carbs, insulin)
    let meals = [(1.0, 45.0, 4.5), (5.5, 70.0, 5.0), (10.5, 55.0, 5.5)];
    let microboluses = [2.2, 2.6, 6.4, 6.8, 7.2, 11.2, 11.5];
    let at = |hours: f32| start + Duration::minutes((hours * 60.0) as i64);

    let mut treatments: Vec<GraphTreatment> = meals
        .iter()
        .map(|&(h, carbs, insulin)| GraphTreatment {
            insulin: Some(insulin),
            carbs: Some(carbs),
            mbg: None,
            date: at(h),
            is_isf: false,
        })
        .collect();
    treatments.extend(microboluses.iter().map(|&h| GraphTreatment {
        insulin: Some(0.3),
        carbs: None,
        mbg: None,
        date: at(h),
        is_isf: true,
    }));
    treatments.push(GraphTreatment {
        insulin: None,
        carbs: Some(15.0),
        mbg: Some(64.0),
        date: at(9.0 + 20.0 / 60.0),
        is_isf: false,
    });

    let readings = (end - start).num_minutes() / 5;
    let entries = (0..=readings)
        .map(|i| {
            let h = i as f32 * 5.0 / 60.0;
            let mut sgv = 120.0 + 6.0 * (h * 2.3).sin() + 3.0 * (h * 7.1).cos();
            for &(mh, carbs, insulin) in &meals {
                sgv += carbs * 2.2 * bump(h - mh, 1.0);
                sgv -= insulin * 14.0 * bump(h - mh, 2.0);
            }
            sgv -= 30.0 * bump(h - 7.6, 1.2);
            sgv += 25.0 * bump(h - 9.33, 0.7);
            GraphEntry {
                sgv: sgv.round(),
                date: start + Duration::minutes(i * 5),
            }
        })
        .collect();

    (entries, treatments)
}

/// The `/bg` card at the end of the sample day.
fn sample_card(start: DateTime<Utc>, end: DateTime<Utc>) -> BgCardData {
    let (entries, _) = simulate_day(start, end);
    let window = Duration::hours(3);
    let status = |sgv: f32| {
        if sgv < TARGETS.0 {
            GlucoseStatus::Low
        } else if sgv > TARGETS.1 {
            GlucoseStatus::High
        } else {
            GlucoseStatus::InRange
        }
    };

    let sparkline_points = entries
        .iter()
        .filter(|e| e.date >= end - window)
        .map(|e| SparklinePoint {
            t: 1.0 - (end - e.date).num_minutes() as f32 / window.num_minutes() as f32,
            sgv: e.sgv,
            status: status(e.sgv),
        })
        .collect();

    let current = entries.last().map_or(120.0, |e| e.sgv);
    let previous = entries.iter().rev().nth(1).map_or(current, |e| e.sgv);
    let delta = current - previous;
    let trend_arrow = match delta / 5.0 {
        r if r >= 2.0 => "↑",
        r if r >= 1.0 => "↗",
        r if r <= -2.0 => "↓",
        r if r <= -1.0 => "↘",
        _ => "→",
    };

    BgCardData {
        current_sgv: current,
        status: status(current),
        trend_arrow: trend_arrow.to_owned(),
        delta: Some(delta),
        watermark_str: "Your Nightscout".to_owned(),
        age_str: "2 min ago".to_owned(),
        time_str: end.format("%H:%M").to_string(),
        iob_str: Some("IOB 2.4u".to_owned()),
        cob_str: Some("COB 21g".to_owned()),
        sparkline_points,
        info_pill: None,
    }
}

/// A week of 5-minute readings: a daily rhythm, meal swings with the odd
/// spike, overnight dips and slow drift. About 72% in range, with a little of
/// every other band.
fn sample_week(end: DateTime<Utc>) -> Vec<GraphEntry> {
    let count = 7 * 288;
    (0..count)
        .map(|i| {
            let mins = i as f32 * 5.0;
            let day = (mins / 1440.0) * std::f32::consts::TAU;
            let meals = (mins / 280.0) * std::f32::consts::TAU;
            let drift = (mins / 3900.0).sin() * 30.0;
            let spike = 60.0 * meals.sin().max(0.0).powi(8) * (1.0 + (mins / 2100.0).sin());
            let dip = 18.0 * (-day.sin()).max(0.0).powi(3) * (1.0 + (mins / 1700.0).sin());
            let sgv = 125.0 + day.sin() * 28.0 + meals.sin() * 38.0 + drift + spike - dip;
            GraphEntry {
                sgv: sgv.clamp(40.0, 350.0).round(),
                date: end - Duration::minutes(((count - 1 - i) * 5) as i64),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn every_image_draws() {
        for image in [
            Showcase::Graph,
            Showcase::Card,
            Showcase::TimeInRange,
            Showcase::ThemedCard,
        ] {
            let png = image
                .png()
                .await
                .unwrap_or_else(|| panic!("{} did not draw", image.filename()));
            assert!(png.starts_with(b"\x89PNG"), "{}", image.filename());
        }
    }
}
