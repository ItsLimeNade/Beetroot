use crate::data::{Context, Error};
use crate::utils::duration_parser::parse_ago_duration;
use crate::utils::sticker_assets;
use crate::utils::theme_assets;
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
    #[description = "Look back in time (e.g. '30s', '2h', '1d', '1w', '1mo', '1y'). The graph ends at this point"]
    #[rename = "at"]
    at_str: Option<String>,
) -> Result<(), Error> {
    let target_user = user.as_ref().unwrap_or(ctx.author());
    let target_id = target_user.id;

    let user_data = get_db_user!(ctx, target_id.get());

    check_privacy!(ctx, target_id, user_data);

    // Honor the data owner's force_ephemeral preference: a private user's glucose
    // is never broadcast publicly, even when someone else views it.
    let reply_ephemeral = user_data.force_ephemeral;

    let client = get_nightscout_client!(ctx, user_data);

    crate::tips::safe_defer_with(ctx, reply_ephemeral).await?;

    let lookback = if let Some(ref s) = at_str {
        match parse_ago_duration(s) {
            Some(d) => Some(d),
            None => {
                tracing::debug!(input = %s, "could not parse 'at' duration");
                send_error!(
                    ctx,
                    "Invalid Time",
                    "Could not parse the time. Use formats like `30s`, `30m`, `2h`, `1d`, `1w`, `1mo`, `1y`, or combinations like `1h30m`."
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
    tracing::debug!(
        target_user = %crate::logging::redact(target_id.get()),
        hours = duration_hours,
        has_lookback = lookback.is_some(),
        "rendering glucose graph"
    );

    let (entries, treatments, profiles) =
        fetch_graph_data!(ctx, client, start_time, graph_end_time);
    tracing::debug!(
        entries = entries.len(),
        treatments = treatments.len(),
        has_profiles = profiles.is_some(),
        "fetched graph data"
    );

    if entries.is_empty() {
        tracing::debug!("no SGV entries in window");
        send_error!(
            ctx,
            "No Data",
            "No glucose entries found for the specified time range."
        );
        return Ok(());
    }

    // Extract targets, timezone, and unit preference from profile
    let (target_low, target_high, user_tz, is_mmol) = profiles
        .as_ref()
        .and_then(|p| p.first())
        .and_then(|p| p.store.get(&p.default_profile_name))
        .map(|store| {
            let low = store.target_low.first().map(|x| x.value).unwrap_or(4.0);
            let high = store.target_high.first().map(|x| x.value).unwrap_or(10.0);
            let tz: Tz = store.timezone.parse().unwrap_or(chrono_tz::UTC);
            let mmol = store.units.starts_with("mmol");
            let (low_mg, high_mg) = if mmol {
                (low * 18.0, high * 18.0)
            } else {
                (low, high)
            };
            (low_mg as f32, high_mg as f32, tz, mmol)
        })
        .unwrap_or((72.0, 180.0, chrono_tz::UTC, false));

    tracing::debug!(tz = %user_tz, is_mmol, "resolved profile settings");
    crate::log_medical!(target_low, target_high, is_mmol, "graph target range");

    let db = &ctx.data().database;
    let theme =
        theme_assets::resolve_user_theme(db, target_id.get(), user_data.active_theme.as_deref())
            .await;
    let user_stickers = db.get_all_user_stickers(target_id.get()).await?;
    let bonbon_stickers = sticker_assets::load_bonbon_stickers(&user_stickers).await;
    tracing::debug!(stickers = bonbon_stickers.len(), "assets resolved, rendering image");

    let graph_width: u32 = 1275 * 2;
    let graph_height: u32 = 825 * 2;
    let has_lookback = lookback.is_some();
    let custom_start = if has_lookback {
        Some(graph_end_time - Duration::hours(duration_hours))
    } else {
        None
    };

    let graph_image = tokio::task::spawn_blocking(move || {
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
            .with_theme(theme)
            .with_units(UnitDisplay::Dual {
                primary: if is_mmol {
                    UnitPreference::MmolL
                } else {
                    UnitPreference::MgDl
                },
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

        if !bonbon_stickers.is_empty() {
            let stickers = StickerSet::new(bonbon_stickers.len().min(8))
                .with_stickers(bonbon_stickers)
                .with_graph_size_ratio(0.22)
                .with_graph_alpha(0.4);
            builder = builder.with_stickers(stickers);
        }

        builder.build().map_err(|e| anyhow::anyhow!(e.to_string()))
    })
    .await??;

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

    ctx.send(
        poise::CreateReply::default()
            .attachment(attachment)
            .ephemeral(reply_ephemeral),
    )
    .await?;

    Ok(())
}
