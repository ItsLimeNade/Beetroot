use crate::data::{Context, Error};
use crate::utils::graph_render::ProfileSettings;
use crate::utils::period::{Period, Picker};
use crate::utils::render;
use crate::utils::theme_assets;
use bonbon::prelude::*;
use chrono::Utc;
use image::ImageEncoder;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::CreateAttachment;

/// Most readings fetched for the period, and again for the one before it.
const ENTRY_LIMIT: usize = 120_000;

/// How the readings are split into columns.
#[derive(Debug, Clone, Copy, poise::ChoiceParameter)]
pub enum BreakdownChoice {
    #[name = "Hour of the day"]
    Hour,
    #[name = "Day of the week"]
    Weekday,
}

impl BreakdownChoice {
    fn grouping(self) -> Grouping {
        match self {
            Self::Hour => Grouping::Hour,
            Self::Weekday => Grouping::Weekday,
        }
    }

    /// The periods that suit this split: days for hours, whole weeks for
    /// weekdays.
    fn picker(self) -> Picker {
        match self {
            Self::Hour => Picker::BY_HOUR,
            Self::Weekday => Picker::BY_WEEKDAY,
        }
    }

    /// The period broken down when none is given. Weekdays get four weeks:
    /// two would leave each column resting on just two days.
    fn default_period(self) -> Period {
        match self {
            Self::Hour => Period::LastDays(14),
            Self::Weekday => Period::LastDays(28),
        }
    }
}

/// Suggests periods for the split picked in the command's `by` option (hours
/// when it is not filled in yet).
async fn autocomplete_period(ctx: Context<'_>, partial: &str) -> Vec<String> {
    let by_weekday = match ctx {
        poise::Context::Application(app) => app.args.iter().any(|option| {
            // Choices arrive as their position in the list.
            option.name == "by"
                && matches!(
                    option.value,
                    serenity::ResolvedValue::Integer(index) if index == BreakdownChoice::Weekday as i64
                )
        }),
        poise::Context::Prefix(_) => false,
    };
    let by = if by_weekday {
        BreakdownChoice::Weekday
    } else {
        BreakdownChoice::Hour
    };
    by.picker().suggestions(partial, Utc::now().date_naive())
}

/// Breaks your time in range down by hour of the day or day of the week.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel",
    user_cooldown = 10
)]
#[track_analytics("breakdown")]
pub async fn breakdown(
    ctx: Context<'_>,
    #[description = "Split by hour of the day or by day of the week (default: hour)"] by: Option<
        BreakdownChoice,
    >,
    #[description = "How far back, or a month: 'Last 4 weeks', 'July 2026'... (default: 14 days, or 4 weeks by day)"]
    #[autocomplete = "autocomplete_period"]
    period: Option<String>,
    #[description = "View another user's breakdown"] user: Option<serenity::User>,
) -> Result<(), Error> {
    let by = by.unwrap_or(BreakdownChoice::Hour);
    let grouping = by.grouping();
    let picker = by.picker();
    let period = match period.as_deref() {
        None => by.default_period(),
        Some(input) => match picker.read(input, Utc::now().date_naive()) {
            Ok(period) => period,
            Err(message) => {
                tracing::debug!(input = %input, "could not read breakdown period");
                send_error!(ctx, "Invalid Period", message);
                return Ok(());
            }
        },
    };
    let period_label = picker.label(period);

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

    let now = Utc::now();
    let (start_time, end_time) = period.bounds(now, settings.timezone);
    let (previous_start, previous_end) = period.previous_bounds(now, settings.timezone);
    tracing::debug!(
        target_user = %crate::logging::redact(target_id.get()),
        period = %period_label,
        grouping = ?grouping,
        "rendering breakdown graph"
    );

    // Each period is its own request: Nightscout returns the newest matches
    // first, so one request for both could crowd the older period out.
    let sgv = client.entries().sgv();
    let (entries_res, previous_res) = tokio::join!(
        sgv.list()
            .since(start_time)
            .until(end_time)
            .limit(ENTRY_LIMIT)
            .send(),
        sgv.list()
            .since(previous_start)
            .until(previous_end)
            .limit(ENTRY_LIMIT)
            .send()
    );

    let entries = match entries_res {
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

    // The period before only adds the "change" chips: without it the
    // breakdown is drawn on its own.
    let previous = match previous_res {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "failed to fetch the previous period");
            Vec::new()
        }
    };

    crate::log_medical!(
        target_low = settings.target_low,
        target_high = settings.target_high,
        is_mmol = settings.is_mmol,
        "breakdown target range"
    );

    let db = &ctx.data().database;
    let theme =
        theme_assets::resolve_user_theme(db, target_id.get(), user_data.active_theme.as_deref())
            .await;

    let title = match grouping {
        Grouping::Hour => format!("Time in range by hour · {period_label}"),
        Grouping::Weekday => format!("Time in range by day · {period_label}"),
    };

    let breakdown_image = render::run_blocking(move || {
        let mut builder = BreakdownGraphBuilder::new()
            .with_layout(LayoutConfig {
                width: 1275 * 2,
                height: 825 * 2,
                ..Default::default()
            })
            .with_grouping(grouping)
            .with_title(title)
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

        if !previous.is_empty() {
            builder = builder.with_previous(previous);
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
            &breakdown_image,
            breakdown_image.width(),
            breakdown_image.height(),
            image::ExtendedColorType::Rgba8,
        )?;
        Ok::<Vec<u8>, anyhow::Error>(buffer)
    })
    .await?;

    let attachment = CreateAttachment::bytes(img_buffer, "breakdown.png");

    ctx.send(
        poise::CreateReply::default()
            .attachment(attachment)
            .ephemeral(reply_ephemeral),
    )
    .await?;

    Ok(())
}
