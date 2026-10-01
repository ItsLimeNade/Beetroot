use crate::data::Error;
use crate::utils::denoise::{self, Strength};
use crate::utils::graph_data;
use crate::utils::render;
use crate::utils::sticker_assets;
use crate::utils::targets::resolve_profile_targets_mgdl;
use crate::utils::theme_assets;
use beetroot_core::Database;
use beetroot_core::models::UserDecrypted;
use bonbon::prelude::*;
use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use cinnamon::model::{DeviceStatus, ProfileStore, Sgv, Treatment};
use image::ImageEncoder;

/// What a graph needs from the data owner's Nightscout profile.
#[derive(Debug, Clone, Copy)]
pub struct ProfileSettings {
    /// Target range in mg/dL.
    pub target_low: f32,
    pub target_high: f32,
    pub timezone: Tz,
    pub is_mmol: bool,
    /// Duration of insulin action, in hours.
    pub dia_hours: Option<f64>,
}

impl ProfileSettings {
    /// Reads the default profile, falling back to 72-180 mg/dL in UTC when
    /// there is none.
    pub fn from_profile(profile: Option<&ProfileStore>) -> Self {
        profile
            .and_then(|p| p.default_entry())
            .map(|store| {
                let (target_low, target_high, is_mmol) = resolve_profile_targets_mgdl(store);
                Self {
                    target_low,
                    target_high,
                    timezone: store
                        .timezone
                        .as_deref()
                        .and_then(|tz| tz.parse().ok())
                        .unwrap_or(chrono_tz::UTC),
                    is_mmol,
                    dia_hours: store.dia,
                }
            })
            .unwrap_or(Self {
                target_low: 72.0,
                target_high: 180.0,
                timezone: chrono_tz::UTC,
                is_mmol: false,
                dia_hours: None,
            })
    }
}

/// Everything Nightscout holds for one graph's time window.
pub struct WindowData {
    pub entries: Vec<Sgv>,
    pub treatments: Vec<Treatment>,
    pub device_statuses: Vec<DeviceStatus>,
}

/// Fetches entries, treatments, and device statuses between `start` and `end`
/// in parallel.
///
/// Treatments reach back before `start` so earlier doses and meals still count
/// toward IOB/COB. Only failing to fetch entries is an error; treatments and
/// device statuses fail gracefully (empty).
pub async fn fetch_window(
    client: &cinnamon::Client,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<WindowData, cinnamon::Error> {
    // Nightscout returns the newest matches first, so without an upper bound a
    // window far in the past would be crowded out by everything after it.
    let entries_fut = client
        .entries()
        .sgv()
        .list()
        .since(start)
        .until(end)
        .limit(5000)
        .send();
    let treatments_fut = client
        .treatments()
        .list()
        .since(start - Duration::hours(graph_data::ON_BOARD_LOOKBACK_HOURS))
        .until(end)
        .limit(5000)
        .send();
    let device_statuses_fut = client
        .devicestatus()
        .list()
        .since(start)
        .until(end)
        .limit(2000)
        .send();

    let (entries_res, treatments_res, device_statuses_res) =
        tokio::join!(entries_fut, treatments_fut, device_statuses_fut);

    let treatments = treatments_res.unwrap_or_else(|e| {
        tracing::warn!("Failed to fetch treatments: {}", e);
        Vec::new()
    });
    let device_statuses = device_statuses_res.unwrap_or_else(|e| {
        tracing::warn!("Failed to fetch device statuses: {}", e);
        Vec::new()
    });

    Ok(WindowData {
        entries: entries_res?,
        treatments,
        device_statuses,
    })
}

/// The stretch of time a graph shows.
#[derive(Debug, Clone, Copy)]
pub struct GraphWindow {
    pub start: DateTime<Utc>,
    pub duration: Duration,
    /// Show exactly this window. Otherwise the graph ends at the latest data
    /// (the usual "last N hours" view).
    pub pinned: bool,
}

/// Renders a glucose graph as a PNG, styled with the data owner's theme,
/// stickers and graph preferences.
///
/// `smoothing` is how strongly the readings are denoised, `None` for raw.
pub async fn render_png(
    db: &Database,
    user_data: &UserDecrypted,
    settings: ProfileSettings,
    data: WindowData,
    window: GraphWindow,
    smoothing: Option<Strength>,
) -> Result<Vec<u8>, Error> {
    let GraphWindow {
        start,
        duration,
        pinned,
    } = window;
    let owner_id = user_data.discord_id;
    let theme =
        theme_assets::resolve_user_theme(db, owner_id, user_data.active_theme.as_deref()).await;
    // The data owner's graph preferences.
    let treatment_mode = if user_data.treatment_mode == "timeline" {
        TreatmentDisplayMode::Timeline
    } else {
        TreatmentDisplayMode::Contextual
    };
    let sticker_count = user_data
        .graph_sticker_count
        .clamp(0, crate::commands::graph_stickers::MAX_GRAPH_STICKERS)
        as usize;

    // Stickers are downloaded on every render, so skip that entirely when the
    // owner has turned them off.
    let bonbon_stickers = if sticker_count > 0 {
        let user_stickers = db.get_all_user_stickers(owner_id).await?;
        sticker_assets::load_bonbon_stickers(&user_stickers).await
    } else {
        Vec::new()
    };
    tracing::debug!(
        unique_stickers = bonbon_stickers.len(),
        sticker_count,
        "graph assets and prefs resolved"
    );

    let entries: Vec<GraphEntry> = data.entries.into_iter().map(GraphEntry::from).collect();
    let entries = match smoothing {
        Some(strength) => denoise::denoise(entries, strength),
        None => entries,
    };
    let treatments: Vec<GraphTreatment> = data
        .treatments
        .into_iter()
        .filter_map(|t| GraphTreatment::try_from(t).ok())
        .collect();

    // IOB/COB mini graphs, only the ones with something on board in the window.
    let (reported_iob, reported_cob) = on_board_from_device_statuses(&data.device_statuses);
    let mut mini_graphs = graph_data::mini_graphs(
        reported_iob,
        reported_cob,
        &treatments,
        settings.dia_hours,
        start,
        start + duration,
    );
    tracing::debug!(mini_graphs = mini_graphs.len(), "mini graphs resolved");

    // The data owner's microbolus preferences. Hidden microboluses leave the
    // graph but still count toward IOB.
    let microbolus_threshold = user_data.microbolus_threshold as f32;
    let treatments = if user_data.display_microbolus {
        treatments
    } else {
        let shown = graph_data::hide_microboluses(&treatments, microbolus_threshold);
        let doses = |list: &[GraphTreatment]| list.iter().filter(|t| t.insulin.is_some()).count();
        if doses(&shown) != doses(&treatments) {
            let end = (start + duration).min(Utc::now());
            mini_graphs = graph_data::pin_iob(mini_graphs, &treatments, start, end);
        }
        shown
    };

    // Keeps the peak clear of the top of the plot.
    let scaling = graph_data::y_scaling(&entries, &treatments, start, start + duration);

    let graph_width: u32 = 1275 * 2;
    let graph_height: u32 = 825 * 2;

    let graph_image = render::run_blocking(move || {
        let layout = LayoutConfig {
            width: graph_width,
            height: graph_height,
            ..Default::default()
        };

        let mut builder = GlucoseGraphBuilder::new()
            .with_treatment_mode(treatment_mode)
            .with_microbolus_threshold(microbolus_threshold)
            .with_scaling(scaling)
            .with_trace(false)
            .with_layout(layout)
            .with_theme(theme)
            .with_units(UnitDisplay::Dual {
                primary: if settings.is_mmol {
                    UnitPreference::MmolL
                } else {
                    UnitPreference::MgDl
                },
            })
            .with_targets(settings.target_low, settings.target_high)
            .with_timezone(settings.timezone)
            .add_entries(entries)
            .add_treatments(treatments)
            .with_mini_graphs(mini_graphs)
            .with_time_axis(TimeAxisMode::EquallyDistributed { count: 6 })
            .with_fixed_duration(duration);

        if pinned {
            builder = builder.start_at(start);
        }

        if !bonbon_stickers.is_empty() && sticker_count > 0 {
            let stickers = StickerSet::new(sticker_count)
                .with_stickers(bonbon_stickers)
                .with_graph_size_ratio(0.22)
                .with_graph_alpha(0.5);
            builder = builder.with_stickers(stickers);
        }

        builder.build().map_err(|e| anyhow::anyhow!(e.to_string()))
    })
    .await?;

    let img_buffer = render::run_blocking(move || {
        let mut buffer = Vec::with_capacity(200_000);
        let encoder = image::codecs::png::PngEncoder::new_with_quality(
            &mut buffer,
            image::codecs::png::CompressionType::Level(9),
            image::codecs::png::FilterType::NoFilter,
        );

        encoder.write_image(
            &graph_image,
            graph_image.width(),
            graph_image.height(),
            image::ExtendedColorType::Rgba8,
        )?;
        Ok::<Vec<u8>, anyhow::Error>(buffer)
    })
    .await?;

    Ok(img_buffer)
}
