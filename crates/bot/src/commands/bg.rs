use crate::data::{Context, Error};
use anyhow::Context as AnyhowContext;
use cinnamon::client::NightscoutClient;
use cinnamon::models::properties::PropertyType;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateAttachment, CreateEmbed, CreateEmbedFooter};

#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
pub async fn bg(
    ctx: Context<'_>,
    #[description = "Target user"] user: Option<serenity::User>,
) -> Result<(), Error> {
    let target_user = user.as_ref().unwrap_or(ctx.author());
    let target_id = target_user.id;

    let user_data = get_db_user!(ctx, target_id.get());

    check_privacy!(ctx, target_id, user_data);

    let client = get_nightscout_client!(ctx, user_data);

    ctx.defer().await?;

    let entries_builder = client.entries().sgv();
    let entries_fut = entries_builder.list().limit(2);

    let properties_builder = client
        .properties()
        .get()
        .only(&[PropertyType::Iob, PropertyType::Cob]);
    let properties_fut = properties_builder.send();

    let profiles_builder = client.profiles();
    let profile_fut = profiles_builder.get();

    let (entries_result, properties_result, profile_result) =
        tokio::join!(entries_fut, properties_fut, profile_fut);

    let entries = match entries_result {
        Ok(e) if !e.is_empty() => e,
        _ => {
            send_error!(
                ctx,
                "Fetch Error",
                "Could not fetch blood glucose data. Please check your URL."
            );
            return Ok(());
        }
    };

    let entry = &entries[0];
    let prev_entry = entries.get(1);

    let delta = if let Some(prev) = prev_entry {
        entry.sgv as f64 - prev.sgv as f64
    } else {
        0.0
    };

    let (target_low, target_high) = if let Ok(profiles) = profile_result {
        if let Some(profile) = profiles.first() {
            let default_name = &profile.default_profile_name;
            if let Some(store) = profile.store.get(default_name) {
                let low = store.target_low.first().map(|x| x.value).unwrap_or(80.0);

                let high = store.target_high.first().map(|x| x.value).unwrap_or(180.0);
                (low, high)
            } else {
                (80.0, 180.0)
            }
        } else {
            (80.0, 180.0)
        }
    } else {
        (80.0, 180.0)
    };

    let entry_time = chrono::DateTime::parse_from_rfc3339(&entry.date_string)
        .unwrap_or_else(|_| chrono::Utc::now().into())
        .with_timezone(&chrono::Utc);

    let now = chrono::Utc::now();
    let duration = now.signed_duration_since(entry_time);

    let time_ago = if duration.num_minutes() < 60 {
        format!("{} minutes ago", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{} hours ago", duration.num_hours())
    } else {
        format!("{} days ago", duration.num_days())
    };

    let color = if (entry.sgv as f64) > target_high {
        Colour::from_rgb(227, 177, 11)
    } else if (entry.sgv as f64) < target_low {
        Colour::from_rgb(235, 47, 47)
    } else {
        Colour::from_rgb(87, 189, 79)
    };

    let title = format!("{}'s Nightscout", target_user.name);
    let thumbnail_url = target_user.avatar_url().unwrap_or_default();

    let icon_bytes = tokio::fs::read("assets/images/nightscout_icon.png").await?;
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

    let sgv_val = entry.sgv;
    let mmol_val = entry.sgv as f64 / 18.0;
    let delta_mmol = delta / 18.0;

    let delta_str = format!("{:+}", delta);
    let delta_mmol_str = format!("{:.1}", delta_mmol);
    let delta_mmol_formatted = if delta > 0.0 {
        format!("+{}", delta_mmol_str)
    } else {
        delta_mmol_str
    };

    let (mgdl_field, mmol_field) = if is_data_old {
        (
            format!("~~{} ({})~~", sgv_val, delta_str),
            format!("~~{:.1} ({})~~", mmol_val, delta_mmol_formatted),
        )
    } else {
        (
            format!("{} ({})", sgv_val, delta_str),
            format!("{:.1} ({})", mmol_val, delta_mmol_formatted),
        )
    };

    embed = embed
        .field("mg/dL", mgdl_field, true)
        .field("mmol/L", mmol_field, true)
        .field("Trend", entry.direction.as_arrow(), true);

    if let Ok(props) = properties_result {
        if let Some(iob) = props.iob {
            if iob.iob > 0.0 {
                embed = embed.field("IOB", format!("{:.2}u", iob.iob), true);
            }
        }
        if let Some(cob) = props.cob {
            if cob.cob > 0.0 {
                embed = embed.field("COB", format!("{:.0}g", cob.cob), true);
            }
        }
    }

    let mbg_res = client.entries().mbg().list().limit(1).await;

    if let Ok(mbg_list) = mbg_res {
        if let Some(mbg) = mbg_list.first() {
            let mbg_time = chrono::DateTime::parse_from_rfc3339(&mbg.date_string)
                .unwrap_or(now.into())
                .with_timezone(&chrono::Utc);

            let mbg_age = now.signed_duration_since(mbg_time).num_minutes();

            if mbg_age <= 30 {
                let val = mbg.mbg;
                let val_mmol = val as f64 / 18.0;
                embed = embed.field(
                    "Fingerprick",
                    format!(
                        "{:.0} mg/dL ({:.1} mmol/L)\n-# {} min ago",
                        val, val_mmol, mbg_age
                    ),
                    false,
                );
            }
        }
    }

    embed = embed.footer(
        CreateEmbedFooter::new(format!("measured • {time_ago}"))
            .icon_url("attachment://nightscout_icon.png"),
    );

    ctx.send(
        poise::CreateReply::default()
            .embed(embed)
            .attachment(icon_attachment),
    )
    .await?;

    Ok(())
}
