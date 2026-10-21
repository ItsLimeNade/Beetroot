use crate::commands::denoise::DenoiseChoice;
use crate::data::{Context, Error};
use crate::utils::denoise::Strength;
use crate::utils::duration_parser::parse_ago_duration;
use crate::utils::graph_render::{self, GraphWindow, ProfileSettings};
use chrono::{Duration, Utc};
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::CreateAttachment;

#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel",
    user_cooldown = 8
)]
#[track_analytics("graph")]
/// Displays a graph of your blood glucose containing boluses and carb intake.
pub async fn graph(
    ctx: Context<'_>,
    #[description = "Hours of data to display (2-24)"]
    #[min = 2]
    #[max = 24]
    hours: i64,
    #[description = "View another user's graph"] user: Option<serenity::User>,
    #[description = "Look back in time (e.g. '30s', '2h', '1d', '1w', '1mo', '1y'). The graph ends at this point"]
    #[rename = "at"]
    at_str: Option<String>,
    #[description = "Smooth sensor noise for this graph only (default: the /denoise setting, off unless changed)"]
    smoothing: Option<DenoiseChoice>,
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

    let profiles_service = client.profiles();
    let (window_res, profiles_res) = tokio::join!(
        graph_render::fetch_window(&client, start_time, graph_end_time),
        profiles_service.current()
    );

    let data = match window_res {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(error = %e, "failed to fetch glucose entries");
            send_error!(
                ctx,
                "Fetch Error",
                "Could not retrieve glucose data. Please try again in a moment."
            );
            return Ok(());
        }
    };
    let profile = match profiles_res {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("Failed to fetch profile: {}", e);
            None
        }
    };
    tracing::debug!(
        entries = data.entries.len(),
        treatments = data.treatments.len(),
        has_profile = profile.is_some(),
        device_statuses = data.device_statuses.len(),
        "fetched graph data"
    );

    if data.entries.is_empty() {
        tracing::debug!("no SGV entries in window");
        send_error!(
            ctx,
            "No Data",
            "No glucose entries found for the specified time range."
        );
        return Ok(());
    }

    // Targets, timezone, unit preference, and insulin duration from the profile
    let settings = ProfileSettings::from_profile(profile.as_ref());

    tracing::debug!(tz = %settings.timezone, is_mmol = settings.is_mmol, "resolved profile settings");
    crate::log_medical!(
        target_low = settings.target_low,
        target_high = settings.target_high,
        is_mmol = settings.is_mmol,
        "graph target range"
    );

    // A one-off choice wins over the data owner's saved /denoise setting.
    let smoothing = match smoothing {
        Some(choice) => choice.strength(),
        None => Strength::from_level(user_data.graph_denoise),
    };

    let duration = Duration::hours(duration_hours);
    let img_buffer = graph_render::render_png(
        &ctx.data().database,
        &user_data,
        settings,
        data,
        GraphWindow {
            start: graph_end_time - duration,
            duration,
            pinned: lookback.is_some(),
        },
        smoothing,
    )
    .await?;

    let attachment = CreateAttachment::bytes(img_buffer, "graph.png");

    ctx.send(
        poise::CreateReply::default()
            .attachment(attachment)
            .ephemeral(reply_ephemeral),
    )
    .await?;

    Ok(())
}
