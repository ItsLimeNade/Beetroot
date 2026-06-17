use crate::data::{Context, Error};
use crate::utils::emojis;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateEmbed};

/// View all of your current bot settings.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("settings")]
pub async fn settings(ctx: Context<'_>) -> Result<(), Error> {
    let user_id = ctx.author().id.get();
    let user_data = get_db_user!(ctx, user_id);
    tracing::debug!(
        user = %crate::logging::redact(user_id),
        is_private = user_data.is_private,
        has_url = user_data.nightscout_url.as_deref().is_some_and(|s| !s.trim().is_empty()),
        has_token = user_data.nightscout_token.is_some(),
        allowed = user_data.allowed_people.len(),
        blocked = user_data.blocked_people.len(),
        "settings viewed"
    );

    let url_display = user_data
        .nightscout_url
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("(not set)")
        .to_string();

    let token_display = if user_data.nightscout_token.is_some() {
        "Set (encrypted)"
    } else {
        "Not set"
    };

    let (privacy_icon, privacy_display) = if user_data.is_private {
        (emojis::lock_closed(), "Private")
    } else {
        (emojis::lock_open(), "Public")
    };

    let allowed = format_user_list(&user_data.allowed_people);
    let blocked = format_user_list(&user_data.blocked_people);

    let embed = CreateEmbed::new()
        .title(format!("{} Your Settings", emojis::nutrition()))
        .color(Colour::from_rgb(87, 189, 79))
        .field(
            format!("{} Nightscout URL", emojis::wifi()),
            format!("`{}`", url_display),
            false,
        )
        .field(
            format!("{} Nightscout Token", emojis::password()),
            token_display,
            true,
        )
        .field(format!("{} Privacy", privacy_icon), privacy_display, true)
        .field(
            format!("{} Allowed Users", emojis::add_user()),
            allowed,
            false,
        )
        .field(
            format!("{} Blocked Users", emojis::wifi_locked()),
            blocked,
            false,
        )
        .field(
            format!("{} Microbolus Threshold", emojis::micro_bolus()),
            format!("{} U", user_data.microbolus_threshold),
            true,
        )
        .field(
            format!("{} Display Microbolus", emojis::micro_bolus()),
            bool_label(user_data.display_microbolus),
            true,
        )
        .field(
            format!("{} Force Ephemeral", emojis::lock_closed()),
            bool_label(user_data.force_ephemeral),
            true,
        )
        .field(
            format!("{} BG Image Mode", emojis::image_mode()),
            bool_label(user_data.bg_image_mode),
            true,
        )
        .field(
            format!("{} Fingerprick Expiry", emojis::date_valid()),
            format!("{} min", user_data.mbg_expiry_time),
            true,
        );

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}

fn bool_label(v: bool) -> &'static str {
    if v { "On" } else { "Off" }
}

fn format_user_list(ids: &[u64]) -> String {
    if ids.is_empty() {
        return "(none)".to_string();
    }
    ids.iter()
        .map(|id| format!("<@{}>", id))
        .collect::<Vec<_>>()
        .join(", ")
}
