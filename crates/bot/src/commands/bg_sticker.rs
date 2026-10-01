use crate::data::{Context, Error};
use crate::utils::emojis;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateEmbed};

/// Toggle whether /bg shows a reaction sticker instead of your profile picture.
#[poise::command(
    slash_command,
    rename = "bg-sticker",
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("bg_sticker")]
pub async fn bg_sticker(
    ctx: Context<'_>,
    #[description = "Show a sticker reacting to your blood sugar on /bg"] enabled: bool,
) -> Result<(), Error> {
    let user_id = ctx.author().id.get();
    let _ = get_db_user!(ctx, user_id);

    let db = &ctx.data().database;
    db.set_bg_sticker(user_id, enabled).await?;
    tracing::info!(
        user = %crate::logging::redact(user_id),
        enabled,
        "bg sticker updated"
    );

    let label = if enabled { "On" } else { "Off" };
    let body = if enabled {
        "/bg will now show one of your stickers as a reaction to your blood sugar instead of your profile picture (you need stickers added with `/add-sticker`)."
    } else {
        "/bg will show your profile picture."
    };

    let embed = CreateEmbed::new()
        .title(format!("{} BG Sticker", emojis::sticker_add()))
        .description(format!("**{}**\n{}", label, body))
        .color(Colour::DARK_GREEN);

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}
