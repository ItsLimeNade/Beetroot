use crate::data::{Context, Error};
use crate::utils::theme_assets;
use bonbon::prelude::*;
use chrono::{Duration, Utc};
use image::ImageEncoder;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::CreateAttachment;

/// How far back the Time-in-Range card looks.
#[derive(Debug, Clone, Copy, poise::ChoiceParameter)]
pub enum TirPeriodChoice {
    #[name = "Last 7 days"]
    Week,
    #[name = "Last 14 days"]
    Fortnight,
    #[name = "Last 30 days"]
    Month,
    #[name = "Last 90 days"]
    Quarter,
}

impl TirPeriodChoice {
    fn days(self) -> i64 {
        match self {
            Self::Week => 7,
            Self::Fortnight => 14,
            Self::Month => 30,
            Self::Quarter => 90,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Week => "Last 7 days",
            Self::Fortnight => "Last 14 days",
            Self::Month => "Last 30 days",
            Self::Quarter => "Last 90 days",
        }
    }
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
    #[description = "How far back to summarize"] period: TirPeriodChoice,
    #[description = "View another user's Time in Range"] user: Option<serenity::User>,
) -> Result<(), Error> {
    let target_user = user.as_ref().unwrap_or(ctx.author());
    let target_id = target_user.id;

    let user_data = get_db_user!(ctx, target_id.get());

    check_privacy!(ctx, target_id, user_data);

    // Honor the data owner's force_ephemeral preference (see graph.rs).
    let reply_ephemeral = user_data.force_ephemeral;

    let client = get_nightscout_client!(ctx, user_data);

    crate::tips::safe_defer_with(ctx, reply_ephemeral).await?;

    let now = Utc::now();
    let start_time = now - Duration::days(period.days());
    tracing::debug!(
        target_user = %crate::logging::redact(target_id.get()),
        period_days = period.days(),
        "rendering time-in-range card"
    );

    let entries = match client
        .sgv()
        .get()
        .from(start_time)
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
            format!(
                "No glucose entries found in the {}.",
                period.label().to_lowercase()
            )
        );
        return Ok(());
    }

    let profiles = client.profiles().get().await.ok();
    let (target_low, target_high, is_mmol) = profiles
        .as_ref()
        .and_then(|p| p.first())
        .and_then(|p| p.store.get(&p.default_profile_name))
        .map(|store| {
            let low = store.target_low.first().map(|x| x.value).unwrap_or(4.0);
            let high = store.target_high.first().map(|x| x.value).unwrap_or(10.0);
            let mmol = store.units.starts_with("mmol");
            let (low_mg, high_mg) = if mmol {
                (low * 18.0, high * 18.0)
            } else {
                (low, high)
            };
            (low_mg as f32, high_mg as f32, mmol)
        })
        .unwrap_or((72.0, 180.0, false));

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

    let period_label = period.label().to_string();

    let tir_image = tokio::task::spawn_blocking(move || {
        let graph_entries: Vec<GraphEntry> = entries.into_iter().map(GraphEntry::from).collect();

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
