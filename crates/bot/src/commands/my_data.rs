use crate::data::{Context, Error};
use crate::utils::emojis;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateEmbed, CreateEmbedFooter};

fn yes_no(value: bool) -> &'static str {
    if value { "Yes" } else { "No" }
}

/// Render a Unix timestamp (seconds) as a Discord long-date, localized per viewer.
fn date(ts: i64) -> String {
    format!("<t:{ts}:D>")
}

/// Show every piece of data Beetroot stores about you (a data request).
#[poise::command(
    slash_command,
    rename = "my-data",
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("my_data")]
pub async fn my_data(ctx: Context<'_>) -> Result<(), Error> {
    let user_id = ctx.author().id.get();
    let db = &ctx.data().database;

    let Some(user) = db.get_user(user_id).await? else {
        let embed = CreateEmbed::new()
            .title(format!("{} No Data Stored", emojis::wifi_off()))
            .description(
                "Beetroot has nothing stored about you. Run `/setup` to get started.",
            )
            .color(Colour::LIGHT_GREY);
        ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
            .await?;
        return Ok(());
    };

    let summary = db.get_user_data_summary(user_id).await?;

    // Profile: what you configured. The token is encrypted at rest and is never
    // shown; we only reveal whether one is stored.
    let profile = format!(
        "**Nightscout URL:** {}\n**Access token stored:** {}\n**Privacy:** {}\n**Allowed users:** {}\n**Blocked users:** {}",
        user.nightscout_url.as_deref().unwrap_or("Not set"),
        yes_no(user.nightscout_token.is_some()),
        if user.is_private { "Private" } else { "Public" },
        user.allowed_people.len(),
        user.blocked_people.len(),
    );

    let preferences = format!(
        "**Active theme:** {}\n**Treatment mode:** {}\n**Show microboluses:** {} (threshold {} U)\n**Image mode:** {}\n**Fingerprick expiry:** {} s\n**Graph stickers:** {}\n**Always ephemeral:** {}",
        user.active_theme.as_deref().unwrap_or("Default"),
        user.treatment_mode,
        yes_no(user.display_microbolus),
        user.microbolus_threshold,
        yes_no(user.bg_image_mode),
        user.mbg_expiry_time,
        user.graph_sticker_count,
        yes_no(user.force_ephemeral),
    );

    let consent = match user.telemetry_accepted {
        Some(true) => "Enabled",
        Some(false) => "Disabled",
        None => "Not set (off by default)",
    };

    let mut telemetry = format!(
        "**Consent:** {}\n**Commands recorded:** {}",
        consent, summary.command_log_count,
    );
    if let (Some(first), Some(last)) = (summary.first_at, summary.last_at) {
        telemetry.push_str(&format!("\n**First:** {}\n**Latest:** {}", date(first), date(last)));
    }
    if !summary.per_command.is_empty() {
        let top: Vec<String> = summary
            .per_command
            .iter()
            .take(5)
            .map(|(name, count)| format!("`/{name}` x{count}"))
            .collect();
        telemetry.push_str(&format!("\n**Most used:** {}", top.join(", ")));
    }

    let embed = CreateEmbed::new()
        .title(format!("{} Your Beetroot Data", emojis::password()))
        .description(format!(
            "Everything stored about you, <@{user_id}>. Your access token is encrypted and never shown."
        ))
        .field("Profile", profile, false)
        .field("Preferences", preferences, false)
        .field("Usage telemetry", telemetry, false)
        .footer(CreateEmbedFooter::new(
            "Change telemetry with /telemetry - erase everything with /delete-account",
        ))
        .color(Colour::BLURPLE);

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}
