use crate::data::{Context, Error};
use crate::utils::graph_render::ProfileSettings;
use crate::utils::period::{self, Period};
use crate::utils::theme_assets;
use bonbon::prelude::*;
use chrono::Utc;
use image::ImageEncoder;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::CreateAttachment;

/// Suggests the rolling periods, then calendar months, narrowing as the user
/// types (e.g. "July 202" lists every July).
async fn autocomplete_period(_ctx: Context<'_>, partial: &str) -> Vec<String> {
    period::suggestions(partial, Utc::now().date_naive())
}

/// Shows your Time in Range distribution over a chosen period.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel",
    user_cooldown = 10
)]
#[track_analytics("tir")]
pub async fn tir(
    ctx: Context<'_>,
    #[description = "How far back, or a month: 'Last 7 days', 'July 2026'..."]
    #[autocomplete = "autocomplete_period"]
    period: String,
    #[description = "View another user's Time in Range"] user: Option<serenity::User>,
) -> Result<(), Error> {
    let Some(period) = period::parse(&period, Utc::now().date_naive()) else {
        tracing::debug!(input = %period, "could not parse TIR period");
        send_error!(
            ctx,
            "Invalid Period",
            "Pick a period from the list, or type one like `Last 7 days`, `45d`, `July 2026` or `2026-07`. Periods go up to 90 days, and months can't be in the future."
        );
        return Ok(());
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
    let profiles = client.profiles().get().await.ok();
    let settings = ProfileSettings::from_profiles(profiles.as_deref());
    let (target_low, target_high, is_mmol) =
        (settings.target_low, settings.target_high, settings.is_mmol);

    let now = Utc::now();
    let (start_time, end_time) = period.bounds(now, settings.timezone);
    tracing::debug!(
        target_user = %crate::logging::redact(target_id.get()),
        period = %period_label,
        "rendering time-in-range card"
    );

    let entries = match client
        .sgv()
        .get()
        .from(start_time)
        // Nightscout returns the newest matches first, so a past month needs
        // its end bound or everything after it would crowd it out.
        .to(end_time)
        .limit(120_000)
        .send()
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

    tracing::debug!(is_mmol, "resolved target range from profile");
    crate::log_medical!(target_low, target_high, is_mmol, "TIR target range");

    let db = &ctx.data().database;
    let theme =
        theme_assets::resolve_user_theme(db, target_id.get(), user_data.active_theme.as_deref())
            .await;

    tracing::debug!(
        theme = user_data.active_theme.as_deref().unwrap_or("default"),
        "assets resolved, rendering image"
    );

    let tir_image = tokio::task::spawn_blocking(move || {
        let graph_entries: Vec<GraphEntry> = entries
            .into_iter()
            .map(crate::utils::graph_data::graph_entry)
            .collect();

        let builder = TimeInRangeBuilder::new()
            .with_entries(graph_entries)
            .with_targets(target_low, target_high)
            .with_units(UnitDisplay::Dual {
                primary: if is_mmol {
                    UnitPreference::MmolL
                } else {
                    UnitPreference::MgDl
                },
            })
            .with_period_label(period_label)
            .with_extremes(true)
            .with_theme(theme)
            .with_scale(2.0);

        builder.build().map_err(|e| anyhow::anyhow!(e.to_string()))
    })
    .await??;

    let img_buffer = tokio::task::spawn_blocking(move || {
        let mut buffer = Vec::with_capacity(120_000);
        let encoder = image::codecs::png::PngEncoder::new_with_quality(
            &mut buffer,
            image::codecs::png::CompressionType::Level(9),
            image::codecs::png::FilterType::NoFilter,
        );

        encoder.write_image(
            &tir_image,
            tir_image.width(),
            tir_image.height(),
            image::ExtendedColorType::Rgba8,
        )?;
        Ok::<Vec<u8>, anyhow::Error>(buffer)
    })
    .await??;

    let attachment = CreateAttachment::bytes(img_buffer, "tir.png");

    ctx.send(
        poise::CreateReply::default()
            .attachment(attachment)
            .ephemeral(reply_ephemeral),
    )
    .await?;

    Ok(())
}
