use crate::commands::tir::autocomplete_period;
use crate::data::{Context, Error};
use crate::utils::graph_render::ProfileSettings;
use crate::utils::period::{self, Period};
use crate::utils::render;
use crate::utils::theme_assets;
use bonbon::prelude::*;
use chrono::Utc;
use image::ImageEncoder;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::CreateAttachment;

/// Period summarized when none is given: the usual choice for a glucose
/// profile.
const DEFAULT_PERIOD: Period = Period::LastDays(14);

/// Shows your typical day: median glucose and its usual range by time of day.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel",
    user_cooldown = 10
)]
#[track_analytics("agp")]
pub async fn agp(
    ctx: Context<'_>,
    #[description = "How far back, or a month: 'Last 30 days', 'July 2026'... (default: last 14 days)"]
    #[autocomplete = "autocomplete_period"]
    period: Option<String>,
    #[description = "View another user's glucose profile"] user: Option<serenity::User>,
) -> Result<(), Error> {
    let period = match period.as_deref() {
        None => DEFAULT_PERIOD,
        Some(input) => match period::parse(input, Utc::now().date_naive()) {
            Some(period) => period,
            None => {
                tracing::debug!(input = %input, "could not parse AGP period");
                send_error!(
                    ctx,
                    "Invalid Period",
                    "Pick a period from the list, or type one like `Last 7 days`, `45d`, `July 2026` or `2026-07`. Periods go up to 90 days, and months can't be in the future."
                );
                return Ok(());
            }
        },
    };
    let period_label = period.label();

    let target_user = user.as_ref().unwrap_or(ctx.author());
    let target_id = target_user.id;

    let user_data = get_db_user!(ctx, target_id.get());

    check_privacy!(ctx, target_id, user_data);

    // Honor the data owner's force_ephemeral preference (see graph.rs).
    let reply_ephemeral = user_data.force_ephemeral;

    let client = get_nightscout_client!(ctx, user_data);

    crate::tips::safe_defer_with(ctx, reply_ephemeral).await?;

    // The profile comes first: a named month starts at midnight in its timezone.
    let profile = client.profiles().current().await.ok().flatten();
    let settings = ProfileSettings::from_profile(profile.as_ref());

    let (start_time, end_time) = period.bounds(Utc::now(), settings.timezone);
    tracing::debug!(
        target_user = %crate::logging::redact(target_id.get()),
        period = %period_label,
        "rendering glucose profile"
    );

    let entries = match client
        .entries()
        .sgv()
        .list()
        .since(start_time)
        // Nightscout returns the newest matches first, so a past month needs
        // its end bound or everything after it would crowd it out.
        .until(end_time)
        .limit(120_000)
        .await
    {
        Ok(e) => {
            tracing::debug!(count = e.len(), "fetched SGV entries");
            e
        }
        Err(e) => {
            tracing::warn!(error = %e, "nightscout SGV request failed");
            send_error!(
                ctx,
                "Fetch Error",
                "Could not retrieve glucose data. Please try again in a moment."
            );
            return Ok(());
        }
    };

    if entries.is_empty() {
        tracing::debug!("no SGV entries in window");
        send_error!(
            ctx,
            "No Data",
            match period {
                Period::LastDays(_) => format!(
                    "No glucose entries found in the {}.",
                    period_label.to_lowercase()
                ),
                Period::Month { .. } => format!("No glucose entries found in {period_label}."),
            }
        );
        return Ok(());
    }

    crate::log_medical!(
        target_low = settings.target_low,
        target_high = settings.target_high,
        is_mmol = settings.is_mmol,
        "AGP target range"
    );

    let db = &ctx.data().database;
    let theme =
        theme_assets::resolve_user_theme(db, target_id.get(), user_data.active_theme.as_deref())
            .await;

    let profile_image = render::run_blocking(move || {
        let builder = PercentileGraphBuilder::new()
            .with_layout(LayoutConfig {
                width: 1275 * 2,
                height: 825 * 2,
                ..Default::default()
            })
            .with_entries(entries)
            .with_targets(settings.target_low, settings.target_high)
            .with_units(UnitDisplay::Dual {
                primary: if settings.is_mmol {
                    UnitPreference::MmolL
                } else {
                    UnitPreference::MgDl
                },
            })
            .with_timezone(settings.timezone)
            .with_theme(theme);

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
            &profile_image,
            profile_image.width(),
            profile_image.height(),
            image::ExtendedColorType::Rgba8,
        )?;
        Ok::<Vec<u8>, anyhow::Error>(buffer)
    })
    .await?;

    let attachment = CreateAttachment::bytes(img_buffer, "agp.png");

    ctx.send(
        poise::CreateReply::default()
            .attachment(attachment)
            .ephemeral(reply_ephemeral),
    )
    .await?;

    Ok(())
}
