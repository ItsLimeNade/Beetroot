use crate::data::{Context, Error};
use crate::utils::emojis;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateEmbed};

/// Always send bot responses as ephemeral (only you can see them).
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("ephemeral")]
pub async fn ephemeral(
    ctx: Context<'_>,
    #[description = "Force every response to be ephemeral"] enabled: bool,
) -> Result<(), Error> {
    let user_id = ctx.author().id.get();
    let _ = get_db_user!(ctx, user_id);

    let db = &ctx.data().database;
    db.set_force_ephemeral(user_id, enabled).await?;

    let label = if enabled { "On" } else { "Off" };
    let body = if enabled {
        "All responses will be sent privately so only you can see them."
    } else {
        "Responses will be visible to everyone in the channel."
    };

    let icon = if enabled {
        emojis::LOCK_CLOSED
    } else {
        emojis::LOCK_OPEN
    };
    let embed = CreateEmbed::new()
        .title(format!("{} Ephemeral Mode", icon))
        .description(format!("**{}**\n{}", label, body))
        .color(Colour::DARK_GREEN);

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}
