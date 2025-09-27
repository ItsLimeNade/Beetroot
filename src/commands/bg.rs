use std::str::FromStr;
use crate::Handler;
use serenity::all::{
    Colour, CommandInteraction, CommandOptionType, Context, CreateEmbed, CreateInteractionResponse, CreateInteractionResponseMessage, InteractionContext};
use serenity::builder::{CreateCommand, CreateCommandOption};

pub async fn run(
    handler: &Handler,
    context: &Context,
    interaction: &CommandInteraction,
) -> anyhow::Result<()> {
    let entry = handler.nightscout_client.get_entry(base_url).await?;
    let delta = handler.nightscout_client.get_current_delta(base_url).await?;

    let profile = handler.nightscout_client.get_profile(base_url).await?;
    let default_profile_name = &profile.default_profile;
    let profile_store = profile.store.get(default_profile_name)
        .ok_or_else(|| anyhow::anyhow!("Default profile not found"))?;

    let user_timezone = &profile_store.timezone;
    let entry_time = entry.millis_to_user_timezone(user_timezone);
    let now = chrono::Utc::now().with_timezone(&chrono_tz::Tz::from_str(user_timezone).unwrap_or(chrono_tz::UTC));
    let duration = now.signed_duration_since(entry_time);

    let time_ago = if duration.num_minutes() < 60 {
        format!("{} minutes ago", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{} hours ago", duration.num_hours())
    } else {
        format!("{} days ago", duration.num_days())
    };

    let color: Colour = if entry.sgv > 180.0 {
        Colour::from_rgb(227, 177, 11)
    } else if entry.sgv < 70.0 {
        Colour::from_rgb(235, 47, 47)
    } else {
        Colour::from_rgb(87, 189, 79)
    };

    let icon_bytes = std::fs::read("assets/images/nightscout_icon.png")?;
    let icon_attachment = serenity::builder::CreateAttachment::bytes(icon_bytes, "nightscout_icon.png");

    let message = CreateInteractionResponseMessage::new()
    .add_embed(
        CreateEmbed::new()
        .thumbnail(interaction.user.avatar_url().unwrap_or(String::from("")))
        .title("Nightscout data")
        .color(color)
        .field("mg/dL", format!("{} ({})", entry.sgv, delta.as_signed_str()), true)
        .field("mmol/L", format!("{} ({})", entry.svg_as_mmol(), delta.as_mmol().as_signed_str()), true)
        .field("Trend", entry.trend().as_arrow(), true)
        .footer(serenity::all::CreateEmbedFooter::new(format!("measured • {time_ago}"))
            .icon_url("attachment://nightscout_icon.png"))
    )
    .add_file(icon_attachment);

    let builder = CreateInteractionResponse::Message(message);
    if let Err(why) = interaction.create_response(&context.http, builder).await {
        println!("Cannot respond to slash command: {why}");
    }
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
