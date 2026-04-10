use crate::data::{Context, Error};
use crate::stickers;
use crate::stickers::overlay::{GraphCoordParams, overlay_stickers_on_graph};
use crate::utils::duration_parser::parse_ago_duration;
use bonbon::prelude::*;
use chrono::{Duration, Utc};
use chrono_tz::Tz;
use image::ImageEncoder;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::CreateAttachment;

#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("graph")]
/// Displays a graph of your blood glucose containing boluses and carb intake.
pub async fn graph(
    ctx: Context<'_>,
    #[description = "Hours of data to display (3-24)"]
    #[min = 3]
    #[max = 24]
    hours: i64,
    #[description = "View another user's graph"] user: Option<serenity::User>,
    #[description = "Look back in time (e.g. '30m', '2h', '1h30m'). The graph ends at this point"]
    #[rename = "at"]
    at_str: Option<String>,
) -> Result<(), Error> {
    let target_user = user.as_ref().unwrap_or(ctx.author());
    let target_id = target_user.id;

    let user_data = get_db_user!(ctx, target_id.get());

    check_privacy!(ctx, target_id, user_data);

    let client = get_nightscout_client!(ctx, user_data);

    ctx.defer().await?;

    let lookback = if let Some(ref s) = at_str {
        match parse_ago_duration(s) {
            Some(d) => Some(d),
            None => {
                send_error!(
                    ctx,
                    "Invalid Time",
                    "Could not parse the time. Use formats like `30m`, `2h`, `1h30m`."
                );
                return Ok(());
            }
        }
    } else {
        None
    };

    let duration_hours = hours;
    let now = Utc::now();

    let graph_end_time = if let Some(ago) = lookback {
        now - ago
    } else {
        now
    };

    let start_time = graph_end_time - Duration::hours(duration_hours) - Duration::minutes(15);

    let (entries, treatments, profiles) =
        fetch_graph_data!(ctx, client, start_time, graph_end_time);

    if entries.is_empty() {
        send_error!(
            ctx,
            "No Data",
            "No glucose entries found for the specified time range."
        );
        return Ok(());
    }

    // Extract targets and timezone from profile
    let (target_low, target_high, user_tz) = profiles
        .as_ref()
        .and_then(|p| p.first())
        .and_then(|p| p.store.get(&p.default_profile_name))
        .map(|store| {
            let low = store.target_low.first().map(|x| x.value).unwrap_or(80.0);
            let high = store.target_high.first().map(|x| x.value).unwrap_or(180.0);
            let tz: Tz = store.timezone.parse().unwrap_or(chrono_tz::UTC);
            (low as f32, high as f32, tz)
        })
        .unwrap_or((80.0, 180.0, chrono_tz::UTC));

    let sgv_values: Vec<f32> = entries.iter().map(|e| e.sgv as f32).collect();
    let entry_times: Vec<chrono::DateTime<Utc>> = entries
        .iter()
        .filter_map(|e| chrono::DateTime::from_timestamp_millis(e.date))
        .collect();
    
    let db = &ctx.data().database;
    let user_stickers = db.get_all_user_stickers(target_id.get()).await?;

    let graph_width: u32 = 2550;
    let graph_height: u32 = 1650;
    let has_lookback = lookback.is_some();
    let custom_start = if has_lookback {
        Some(graph_end_time - Duration::hours(duration_hours))
    } else {
        None
    };

    let mut graph_image = tokio::task::spawn_blocking(move || {
        let layout = LayoutConfig {
            width: graph_width,
            height: graph_height,
            ..Default::default()
        };

        let mut builder = GlucoseGraphBuilder::new()
            .with_treatment_mode(TreatmentDisplayMode::Contextual)
            .with_scaling(GraphScaling::Dynamic {
                clamp_min: 40.0,
                clamp_max: 400.0,
                default_min: 60.0,
                default_max: 200.0,
            })
            .with_layout(layout)
            .with_theme(Theme::dark())
            .with_units(UnitDisplay::Dual {
                primary: UnitPreference::MgDl,
            })
            .with_targets(target_low, target_high)
            .with_timezone(user_tz)
            .add_entries(entries)
            .add_treatments(treatments)
            .with_time_axis(TimeAxisMode::EquallyDistributed { count: 6 })
            .with_fixed_duration(Duration::hours(duration_hours));

        if let Some(start) = custom_start {
            builder = builder.start_at(start);
        }

        builder.build().map_err(|e| anyhow::anyhow!(e.to_string()))
    })
    .await??;

    if !user_stickers.is_empty() && !sgv_values.is_empty() {
        let placements = stickers::generate_sticker_placements(
            &sgv_values,
            &user_stickers,
            target_low,
            target_high,
        );

        if !placements.is_empty() {
            let (y_min, y_max) = compute_dynamic_y_range(&sgv_values, 40.0, 400.0, 60.0, 200.0);

            let graph_end = if has_lookback {
                graph_end_time
            } else {
                entry_times.iter().copied().max().unwrap_or(now)
            };
            let graph_start = graph_end - Duration::hours(duration_hours);

            let coord_params = GraphCoordParams {
                width: graph_width,
                height: graph_height,
                start_time: graph_start,
                end_time: graph_end,
                y_min,
                y_max,
                margin_left: None, // use bonbon defaults
                margin_right: None,
                margin_top: None,
                margin_bottom: None,
            };

            overlay_stickers_on_graph(
                &mut graph_image,
                &placements,
                &entry_times,
                &sgv_values,
                &coord_params,
            )
            .await?;
        }
    }

    let img_buffer = tokio::task::spawn_blocking(move || {
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
    .await??;

    let attachment = CreateAttachment::bytes(img_buffer, "graph.png");

    ctx.send(poise::CreateReply::default().attachment(attachment))
        .await?;

    Ok(())
}

/// Reproduce bonbon's Dynamic Y-axis scaling logic so we can accurately
/// project glucose values to pixel coordinates for sticker placement.
///
/// bonbon's Dynamic scaling:
/// - default_min / default_max define the normal visible range
/// - If any entry goes below default_min or above default_max,
///   the range expands to fit (clamped to clamp_min / clamp_max)
pub fn compute_dynamic_y_range(
    sgv_values: &[f32],
    clamp_min: f32,
    clamp_max: f32,
    default_min: f32,
    default_max: f32,
) -> (f32, f32) {
    if sgv_values.is_empty() {
        return (default_min, default_max);
    }

    let data_min = sgv_values.iter().cloned().fold(f32::INFINITY, f32::min);
    let data_max = sgv_values.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

    let y_min = data_min.min(default_min).max(clamp_min);
    let y_max = data_max.max(default_max).min(clamp_max);

    (y_min, y_max)
}
