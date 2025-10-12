use crate::bot::Handler;
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

    let status = handler
        .nightscout_client
        .get_status(base_url, token)
        .await
        .ok();

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

    let now_utc = chrono::Utc::now();
    let thirty_min_ago = now_utc - chrono::Duration::minutes(30);
    let start_time = thirty_min_ago.to_rfc3339();
    let end_time = now_utc.to_rfc3339();

    let recent_entries = handler
        .nightscout_client
        .get_entries_for_hours(base_url, 1, token)
        .await
        .unwrap_or_default();

    let recent_treatments = handler
        .nightscout_client
        .fetch_treatments_between(base_url, &start_time, &end_time, token)
        .await
        .unwrap_or_default();

    let default_profile_name = &profile.default_profile;
    let profile_store = profile
        .store
        .get(default_profile_name)
        .context("Default profile not found")?;

    let thresholds = status
        .as_ref()
        .and_then(|s| s.settings.as_ref())
        .and_then(|settings| settings.thresholds.as_ref());

    let user_timezone = &profile_store.timezone;
    let target_low_mg = profile_store.get_target_low_mg(thresholds);
    let target_high_mg = profile_store.get_target_high_mg(thresholds);

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

    let color = if entry.sgv > target_high_mg {
        Colour::from_rgb(227, 177, 11)
    } else if entry.sgv < target_low_mg {
        Colour::from_rgb(235, 47, 47)
    } else {
        Colour::from_rgb(87, 189, 79)
    };

    let target_user = context.http.get_user(target_user_id.into()).await.ok();
    let thumbnail_url = target_user
        .as_ref()
        .and_then(|u| u.avatar_url())
        .unwrap_or_default();

    let custom_title = status
        .as_ref()
        .and_then(|s| s.settings.as_ref())
        .and_then(|settings| settings.custom_title.as_deref())
        .filter(|title| *title != "Nightscout");

    let title = if let Some(custom) = custom_title {
        custom.to_string()
    } else {
        format!(
            "{}'s Nightscout data",
            target_user
                .as_ref()
                .map(|u| u.display_name())
                .unwrap_or_else(|| "User")
        )
    };

    let icon_bytes = std::fs::read("assets/images/nightscout_icon.png")?;
    let icon_attachment = CreateAttachment::bytes(icon_bytes, "nightscout_icon.png");

    let mut embed = CreateEmbed::new()
        .thumbnail(thumbnail_url)
        .title(title)
        .color(color);

    let is_data_old = duration.num_minutes() > 15;

    if is_data_old {
        embed = embed.field(
            "⚠️ Warning ⚠️",
            format!("Data is {}min old!", duration.num_minutes()),
            false,
        );
    }

    let (mgdl_value, mmol_value) = if is_data_old {
        (
            format!("~~{} ({})~~", entry.sgv, delta.as_signed_str()),
            format!(
                "~~{} ({})~~",
                entry.svg_as_mmol(),
                delta.as_mmol().as_signed_str()
            ),
        )
    } else {
        (
            format!("{} ({})", entry.sgv, delta.as_signed_str()),
            format!(
                "{} ({})",
                entry.svg_as_mmol(),
                delta.as_mmol().as_signed_str()
            ),
        )
    };

    embed = embed
        .field("mg/dL", mgdl_value, true)
        .field("mmol/L", mmol_value, true)
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
    }

    let mut fingerprick_value: Option<(f32, u64)> = None;
    let thirty_min_ago_millis = thirty_min_ago.timestamp_millis() as u64;

    for entry in recent_entries.iter() {
        if entry.has_mbg()
            && let Some(entry_time_millis) = entry.date
            && entry_time_millis >= thirty_min_ago_millis
            && let Some(mbg) = entry.mbg
        {
            fingerprick_value = Some((mbg, entry_time_millis));
            break;
        }
    }

    if fingerprick_value.is_none() {
        for treatment in recent_treatments.iter() {
            if treatment.event_type.as_deref() == Some("BG Check")
                && let Some(glucose_str) = &treatment.glucose
                && let Ok(glucose) = glucose_str.parse::<f32>()
            {
                let treatment_time_millis = if let Some(time) = treatment.date.or(treatment.mills) {
                    time
                } else if let Some(created_at) = &treatment.created_at {
                    if let Ok(parsed_time) = chrono::DateTime::parse_from_rfc3339(created_at) {
                        parsed_time.timestamp_millis() as u64
                    } else {
                        continue;
                    }
                } else {
                    continue;
                };

                if treatment_time_millis >= thirty_min_ago_millis {
                    fingerprick_value = Some((glucose, treatment_time_millis));
                    break;
                }
            }
        }
    }

    if let Some((fp_value, fp_timestamp)) = fingerprick_value {
        let fp_mmol = fp_value / 18.0;

        let now_millis = now_utc.timestamp_millis() as u64;

        tracing::info!("[BG] Fingerprick timestamp: {}", fp_timestamp);
        tracing::info!("[BG] Now timestamp: {}", now_millis);

        let timestamp_millis = if fp_timestamp < 10000000000 {
            tracing::info!("[BG] Converting seconds to milliseconds");
            fp_timestamp * 1000
        } else {
            tracing::info!("[BG] Timestamp already in milliseconds");
            fp_timestamp
        };

        tracing::info!("[BG] Normalized timestamp: {}", timestamp_millis);

        let diff_millis = now_millis.saturating_sub(timestamp_millis);
        tracing::info!("[BG] Difference in milliseconds: {}", diff_millis);

        let fp_age_minutes = diff_millis / 1000 / 60;
        tracing::info!("[BG] Age in minutes: {}", fp_age_minutes);

        embed = embed.field(
            "Fingerprick",
            format!(
                "{:.0} mg/dL ({:.1} mmol/L)\n-# {} min ago",
                fp_value, fp_mmol, fp_age_minutes
            ),
            false,
        );
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
