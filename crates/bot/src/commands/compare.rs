use crate::data::{Context, Error};
use crate::utils::graph_render::ProfileSettings;
use crate::utils::period::{Period, Picker};
use crate::utils::render;
use crate::utils::theme_assets;
use bonbon::prelude::*;
use chrono::{NaiveDate, Utc};
use image::ImageEncoder;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::CreateAttachment;

/// Most readings fetched for each of the two periods.
const ENTRY_LIMIT: usize = 120_000;

/// The period compared when none is given: the usual choice.
const DEFAULT_PERIOD: Period = Period::LastDays(14);

/// How the default for `against` reads in its suggestions.
const AGAINST_BEFORE: &str = "The period before";

/// Discord shows at most 25 autocomplete choices.
const MAX_SUGGESTIONS: usize = 25;

/// What a period is compared against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Against {
    /// The period just before it: the same number of days again, or the
    /// previous calendar month.
    Before,
    /// A calendar month of the user's choosing.
    Month(Period),
}

/// Reads the `against` option. The error is a message fit to show the user.
fn read_against(input: Option<&str>, period: Period, today: NaiveDate) -> Result<Against, String> {
    let Some(input) = input.map(str::trim).filter(|i| !i.is_empty()) else {
        return Ok(Against::Before);
    };
    if [
        "the period before",
        "period before",
        "previous period",
        "before",
    ]
    .contains(&input.to_lowercase().as_str())
    {
        return Ok(Against::Before);
    }

    match Picker::TYPICAL_DAY.read(input, today) {
        Ok(month @ Period::Month { .. }) if month == period => {
            Err("Those are the same month. Pick two different periods to compare.".to_string())
        }
        Ok(month @ Period::Month { .. }) => Ok(Against::Month(month)),
        Ok(Period::LastDays(_)) => Err(format!(
            "`against` takes a month, like `July 2026`. Leave it empty to compare with {}.",
            AGAINST_BEFORE.to_lowercase()
        )),
        Err(message) => Err(message),
    }
}

/// The title of the card for what `period` is compared against.
fn against_title(against: Against, period: Period) -> String {
    match (against, period) {
        (Against::Month(month), _) => month.label(),
        (Against::Before, Period::LastDays(days)) => format!("Previous {days} days"),
        (Against::Before, month) => month.previous().map_or_else(String::new, Period::label),
    }
}

/// Suggests periods a typical day can be drawn from.
async fn autocomplete_period(_ctx: Context<'_>, partial: &str) -> Vec<String> {
    Picker::TYPICAL_DAY.suggestions(partial, Utc::now().date_naive())
}

/// Suggests the default ("the period before"), then calendar months.
async fn autocomplete_against(_ctx: Context<'_>, partial: &str) -> Vec<String> {
    let needle = partial.trim().to_lowercase();
    let months = Picker::TYPICAL_DAY
        .months(Utc::now().date_naive())
        .map(Period::label);

    std::iter::once(AGAINST_BEFORE.to_string())
        .chain(months)
        .filter(|label| label.to_lowercase().contains(&needle))
        .take(MAX_SUGGESTIONS)
        .collect()
}

/// Compares two periods of your glucose: by default the last 14 days and the 14 before.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel",
    user_cooldown = 10
)]
#[track_analytics("compare")]
pub async fn compare(
    ctx: Context<'_>,
    #[description = "A week or more, or a month: 'Last 30 days', 'July 2026'... (default: last 14 days)"]
    #[autocomplete = "autocomplete_period"]
    period: Option<String>,
    #[description = "What to compare it with: a month like 'July 2025' (default: the period just before)"]
    #[autocomplete = "autocomplete_against"]
    against: Option<String>,
    #[description = "View another user's comparison"] user: Option<serenity::User>,
) -> Result<(), Error> {
    let today = Utc::now().date_naive();
    let period = match period.as_deref() {
        None => DEFAULT_PERIOD,
        Some(input) => match Picker::TYPICAL_DAY.read(input, today) {
            Ok(period) => period,
            Err(message) => {
                tracing::debug!(input = %input, "could not read compare period");
                send_error!(ctx, "Invalid Period", message);
                return Ok(());
            }
        },
    };
    let against = match read_against(against.as_deref(), period, today) {
        Ok(against) => against,
        Err(message) => {
            tracing::debug!("could not read what to compare against");
            send_error!(ctx, "Invalid Period", message);
            return Ok(());
        }
    };
    let period_title = period.label();
    let against_title = against_title(against, period);

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
    let period_bounds = period.bounds(now, settings.timezone);
    let against_bounds = match against {
        Against::Before => period.previous_bounds(now, settings.timezone),
        Against::Month(month) => month.bounds(now, settings.timezone),
    };
    tracing::debug!(
        target_user = %crate::logging::redact(target_id.get()),
        period = %period_title,
        against = %against_title,
        "rendering compare graph"
    );

    // Each period is its own request: Nightscout returns the newest matches
    // first, so one request for both could crowd the older period out.
    let sgv = client.entries().sgv();
    let fetch = |(start, end)| sgv.list().since(start).until(end).limit(ENTRY_LIMIT).send();
    let (period_res, against_res) = tokio::join!(fetch(period_bounds), fetch(against_bounds));

    let (period_entries, against_entries) = match (period_res, against_res) {
        (Ok(period), Ok(against)) => (period, against),
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
        period = period_entries.len(),
        against = against_entries.len(),
        "fetched SGV entries"
    );

    let no_data = |title: &str| {
        // "the last 14 days", but "July 2026".
        if title.starts_with("Last ") || title.starts_with("Previous ") {
            format!("the {}", title.to_lowercase())
        } else {
            title.to_string()
        }
    };
    if period_entries.is_empty() {
        send_error!(
            ctx,
            "No Data",
            format!("No glucose entries found in {}.", no_data(&period_title))
        );
        return Ok(());
    }
    if against_entries.is_empty() {
        send_error!(
            ctx,
            "Nothing To Compare With",
            format!(
                "No glucose entries found in {}, so there is nothing to compare {} against.",
                no_data(&against_title),
                no_data(&period_title)
            )
        );
        return Ok(());
    }

    // The earlier period goes on the left, whichever option it came from.
    let (first, first_title, second, second_title) = if against_bounds.0 <= period_bounds.0 {
        (against_entries, against_title, period_entries, period_title)
    } else {
        (period_entries, period_title, against_entries, against_title)
    };

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
            .with_periods(first, second)
            .with_titles(first_title, second_title)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 20).unwrap()
    }

    fn month(year: i32, month: u32) -> Period {
        Period::Month { year, month }
    }

    #[test]
    fn against_defaults_to_the_period_before() {
        let period = Period::LastDays(14);
        assert_eq!(read_against(None, period, today()), Ok(Against::Before));
        assert_eq!(
            read_against(Some("  "), period, today()),
            Ok(Against::Before)
        );
        assert_eq!(
            read_against(Some(AGAINST_BEFORE), period, today()),
            Ok(Against::Before)
        );
    }

    #[test]
    fn against_takes_any_other_month() {
        assert_eq!(
            read_against(Some("July 2025"), Period::LastDays(30), today()),
            Ok(Against::Month(month(2025, 7)))
        );
        // Later than the period is fine too: the cards are ordered by date.
        assert_eq!(
            read_against(Some("October 2026"), month(2026, 7), today()),
            Ok(Against::Month(month(2026, 10)))
        );

        let same = read_against(Some("July 2026"), month(2026, 7), today()).unwrap_err();
        assert!(same.contains("same month"), "{same}");
        let rolling = read_against(Some("Last 30 days"), month(2026, 7), today()).unwrap_err();
        assert!(rolling.contains("takes a month"), "{rolling}");
        assert!(read_against(Some("soon"), month(2026, 7), today()).is_err());
    }

    #[test]
    fn titles_name_what_is_compared() {
        assert_eq!(
            against_title(Against::Before, Period::LastDays(14)),
            "Previous 14 days"
        );
        assert_eq!(
            against_title(Against::Before, month(2026, 1)),
            "December 2025"
        );
        assert_eq!(
            against_title(Against::Month(month(2025, 7)), Period::LastDays(14)),
            "July 2025"
        );
    }
}
