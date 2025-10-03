use crate::Handler;
use anyhow::Context as AnyhowContext;
use serenity::all::{
    Colour, CommandInteraction, CommandOptionType, Context, CreateAttachment, CreateEmbed,
    CreateEmbedFooter, CreateInteractionResponse, CreateInteractionResponseMessage,
    InteractionContext, ResolvedOption, ResolvedValue,
};
use serenity::builder::{CreateCommand, CreateCommandOption};
use std::str::FromStr;

pub async fn run(
    handler: &Handler,
    context: &Context,
    interaction: &CommandInteraction,
) -> anyhow::Result<()> {
    let target_user_id = if let Some(ResolvedOption {
        value: ResolvedValue::User(user, _),
        ..
    }) = interaction.data.options().first()
    {
        user.id.get()
    } else {
        interaction.user.id.get()
    };

    let command_user_id = interaction.user.id.get();

    if !handler.database.user_exists(target_user_id).await? {
        crate::commands::error::run(
            context,
            interaction,
            "The specified user hasn't set up their Nightscout data yet.",
        )
        .await?;
        return Ok(());
    }

    let target_user_data = handler.database.get_user_info(target_user_id).await?;

    #[allow(clippy::if_same_then_else)]
    let can_access = if target_user_id == command_user_id {
        true
    } else if !target_user_data.nightscout.is_private {
        true
    } else {
        target_user_data
            .nightscout
            .allowed_people
            .contains(&command_user_id)
    };

    if !can_access {
        crate::commands::error::run(
            context,
            interaction,
            "This user's blood glucose data is set to private.",
        )
        .await?;
        return Ok(());
    }

    let base_url = target_user_data
        .nightscout
        .nightscout_url
        .as_deref()
        .context("Nightscout URL missing")?;

    if base_url.trim().is_empty() {
        crate::commands::error::run(
            context,
            interaction,
            "Your Nightscout URL is empty. Please run `/setup` to configure it properly.",
        )
        .await?;
        return Ok(());
    }

    let token = target_user_data.nightscout.nightscout_token.as_deref();
    let entry = match handler.nightscout_client.get_entry(base_url, token).await {
        Ok(entry) => entry,
        Err(e) => {
            eprintln!("Failed to get entry for user {}: {}", target_user_id, e);
            crate::commands::error::run(
                context,
                interaction,
                "Could not connect to your Nightscout site. Please check your URL configuration with `/setup`.",
            )
            .await?;
            return Ok(());
        }
    };

    let delta = match handler
        .nightscout_client
        .get_current_delta(base_url, token)
        .await
    {
        Ok(delta) => delta,
        Err(e) => {
            eprintln!("Failed to get delta for user {}: {}", target_user_id, e);
            crate::utils::nightscout::Delta { value: 0.0 }
        }
    };

    let profile = match handler.nightscout_client.get_profile(base_url, token).await {
        Ok(profile) => profile,
        Err(e) => {
            eprintln!("Failed to get profile for user {}: {}", target_user_id, e);
            crate::utils::nightscout::Profile {
                default_profile: "default".to_string(),
                store: std::collections::HashMap::new(),
            }
        }
    };

    let pebble_data = handler
        .nightscout_client
        .get_pebble_data(base_url, token)
        .await
        .ok()
        .flatten();

    let devicestatus = if pebble_data.is_none() {
        handler
            .nightscout_client
            .get_devicestatus(base_url, token)
            .await
            .ok()
            .flatten()
    } else {
        None
    };

    let default_profile_name = &profile.default_profile;
    let profile_store = profile
        .store
        .get(default_profile_name)
        .context("Default profile not found")?;

    let user_timezone = &profile_store.timezone;
    let entry_time = entry.millis_to_user_timezone(user_timezone);
    let now = chrono::Utc::now()
        .with_timezone(&chrono_tz::Tz::from_str(user_timezone).unwrap_or(chrono_tz::UTC));
    let duration = now.signed_duration_since(entry_time);

    let time_ago = if duration.num_minutes() < 60 {
        format!("{} minutes ago", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{} hours ago", duration.num_hours())
    } else {
        format!("{} days ago", duration.num_days())
    };

    let color = if entry.sgv > 180.0 {
        Colour::from_rgb(227, 177, 11)
    } else if entry.sgv < 70.0 {
        Colour::from_rgb(235, 47, 47)
    } else {
        Colour::from_rgb(87, 189, 79)
    };

    let target_user = context.http.get_user(target_user_id.into()).await.ok();
    let thumbnail_url = target_user
        .as_ref()
        .and_then(|u| u.avatar_url())
        .unwrap_or_default();

    let title = format!(
        "{}'s Nightscout data",
        target_user
            .as_ref()
            .map(|u| u.display_name())
            .unwrap_or_else(|| "User")
    );

    let icon_bytes = std::fs::read("assets/images/nightscout_icon.png")?;
    let icon_attachment = CreateAttachment::bytes(icon_bytes, "nightscout_icon.png");

    let mut embed = CreateEmbed::new()
        .thumbnail(thumbnail_url)
        .title(title)
        .color(color)
        .field(
            "mg/dL",
            format!("{} ({})", entry.sgv, delta.as_signed_str()),
            true,
        )
        .field(
            "mmol/L",
            format!(
                "{} ({})",
                entry.svg_as_mmol(),
                delta.as_mmol().as_signed_str()
            ),
            true,
        )
        .field("Trend", entry.trend().as_arrow(), true);

    if let Some(pebble) = pebble_data {
        if let Some(iob_str) = pebble.iob
            && let Ok(iob) = iob_str.parse::<f32>()
            && iob > 0.0
        {
            embed = embed.field("IOB", format!("{:.2}u", iob), true);
        }
        if let Some(cob) = pebble.cob
            && cob > 0.0
        {
            embed = embed.field("COB", format!("{:.0}g", cob), true);
        }
    } else if let Some(status) = devicestatus
        && let Some(openaps) = status.openaps
    {
        if let Some(iob_data) = openaps.iob
            && let Some(iob) = iob_data.iob
            && iob > 0.0
        {
            embed = embed.field("IOB", format!("{:.2}u", iob), true);
        }
        if let Some(suggested) = openaps.suggested
            && let Some(cob) = suggested.COB
            && cob > 0.0
        {
            embed = embed.field("COB", format!("{:.0}g", cob), true);
        }
    }

    embed = embed.footer(
        CreateEmbedFooter::new(format!("measured • {time_ago}"))
            .icon_url("attachment://nightscout_icon.png"),
    );

    let message = CreateInteractionResponseMessage::new()
        .add_embed(embed)
        .add_file(icon_attachment);

    interaction
        .create_response(&context.http, CreateInteractionResponse::Message(message))
        .await?;

    Ok(())
}

pub fn register() -> CreateCommand {
    CreateCommand::new("bg")
        .description("Sends your current blood glucose value.")
        .add_option(
            CreateCommandOption::new(CommandOptionType::User, "user", "Target user.")
                .required(false),
        )
        .contexts(vec![
            InteractionContext::Guild,
            InteractionContext::PrivateChannel,
        ])
}
