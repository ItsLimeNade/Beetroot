use crate::data::{Context, Error};
use crate::utils::emojis;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateEmbed};

/// Toggle whether /bg sends a card image instead of a Discord embed.
#[poise::command(
    slash_command,
    rename = "image-mode",
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("image_mode")]
pub async fn image_mode(
    ctx: Context<'_>,
    #[description = "Send /bg as an image card"] enabled: bool,
) -> Result<(), Error> {
    let user_id = ctx.author().id.get();
    let _ = get_db_user!(ctx, user_id);

    let db = &ctx.data().database;
    db.set_bg_image_mode(user_id, enabled).await?;
    tracing::info!(
        user = %crate::logging::redact(user_id),
        enabled,
        "bg image mode updated"
    );

    let label = if enabled { "On" } else { "Off" };
    let body = if enabled {
        "/bg will now reply with a generated image card."
    } else {
        "/bg will use the standard embed reply."
    };

    let embed = CreateEmbed::new()
        .title(format!("{} BG Image Mode", emojis::IMAGE_MODE))
        .description(format!("**{}**\n{}", label, body))
        .color(Colour::DARK_GREEN);

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}
