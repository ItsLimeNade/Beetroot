use crate::data::{Context, Error};
use crate::utils::graph_render::ProfileSettings;
use crate::utils::render;
use crate::utils::theme_assets;
use bonbon::prelude::*;
use chrono::{Duration, Utc};
use image::ImageEncoder;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::CreateAttachment;

/// Most readings fetched for each of the two periods.
const ENTRY_LIMIT: usize = 120_000;

/// How long each of the two compared periods is.
#[derive(Debug, Clone, Copy, poise::ChoiceParameter)]
pub enum ComparePeriodChoice {
    #[name = "Last 7 days vs the 7 before"]
    Week,
    #[name = "Last 14 days vs the 14 before"]
    Fortnight,
    #[name = "Last 30 days vs the 30 before"]
    Month,
}

impl ComparePeriodChoice {
    fn days(self) -> i64 {
        match self {
            Self::Week => 7,
            Self::Fortnight => 14,
            Self::Month => 30,
        }
    }
}

/// Compares your recent glucose with the period just before it.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel",
    user_cooldown = 10
)]
#[track_analytics("compare")]
pub async fn compare(
    ctx: Context<'_>,
    #[description = "Which periods to compare (default: last 14 days vs the 14 before)"]
    period: Option<ComparePeriodChoice>,
    #[description = "View another user's comparison"] user: Option<serenity::User>,
) -> Result<(), Error> {
    let days = period.unwrap_or(ComparePeriodChoice::Fortnight).days();

    let target_user = user.as_ref().unwrap_or(ctx.author());
    let target_id = target_user.id;

    let user_data = get_db_user!(ctx, target_id.get());

    check_privacy!(ctx, target_id, user_data);

    // Honor the data owner's force_ephemeral preference (see graph.rs).
    let reply_ephemeral = user_data.force_ephemeral;

    let client = get_nightscout_client!(ctx, user_data);

    crate::tips::safe_defer_with(ctx, reply_ephemeral).await?;

    let now = Utc::now();
    let middle = now - Duration::days(days);
    let start = middle - Duration::days(days);
    tracing::debug!(
        target_user = %crate::logging::redact(target_id.get()),
        period_days = days,
        "rendering compare graph"
    );

    // Each period is its own request: Nightscout returns the newest matches
    // first, so one request for both could crowd the older period out.
    let sgv = client.entries().sgv();
    let profiles = client.profiles();
    let (previous_res, recent_res, profile_res) = tokio::join!(
        sgv.list()
            .since(start)
            .until(middle)
            .limit(ENTRY_LIMIT)
            .send(),
        sgv.list().since(middle).limit(ENTRY_LIMIT).send(),
        profiles.current()
    );

    let (previous, recent) = match (previous_res, recent_res) {
        (Ok(previous), Ok(recent)) => (previous, recent),
        (Err(e), _) | (_, Err(e)) => {
            tracing::warn!(error = %e, "nightscout SGV request failed");
            send_error!(
                ctx,
                "Fetch Error",
                "Could not retrieve glucose data. Please try again in a moment."
            );
            return Ok(());
        }
    };
    tracing::debug!(
        previous = previous.len(),
        recent = recent.len(),
        "fetched SGV entries"
    );

    if recent.is_empty() {
        send_error!(
            ctx,
            "No Data",
            format!("No glucose entries found in the last {days} days.")
        );
        return Ok(());
    }
    if previous.is_empty() {
        send_error!(
            ctx,
            "Not Enough History",
            format!(
                "There are no glucose entries from the {days} days before the last {days}, so there is nothing to compare against. Try a shorter period."
            )
        );
        return Ok(());
    }

    let settings = ProfileSettings::from_profile(profile_res.ok().flatten().as_ref());
    crate::log_medical!(
        target_low = settings.target_low,
        target_high = settings.target_high,
        is_mmol = settings.is_mmol,
        "compare target range"
    );

    let db = &ctx.data().database;
    let theme =
        theme_assets::resolve_user_theme(db, target_id.get(), user_data.active_theme.as_deref())
            .await;

    let compare_image = render::run_blocking(move || {
        let builder = CompareGraphBuilder::new()
            .with_layout(LayoutConfig {
                width: 2400,
                height: 1350,
                ..Default::default()
            })
            .with_periods(previous, recent)
            .with_titles(format!("Previous {days} days"), format!("Last {days} days"))
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
            &compare_image,
            compare_image.width(),
            compare_image.height(),
            image::ExtendedColorType::Rgba8,
        )?;
        Ok::<Vec<u8>, anyhow::Error>(buffer)
    })
    .await?;

    let attachment = CreateAttachment::bytes(img_buffer, "compare.png");

    ctx.send(
        poise::CreateReply::default()
            .attachment(attachment)
            .ephemeral(reply_ephemeral),
    )
    .await?;

    Ok(())
}
