use crate::data::{Context, Error};
use crate::utils::emojis;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateEmbed, User};

/// Unblock a user previously blocked from your data.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("unblock")]
pub async fn unblock(
    ctx: Context<'_>,
    #[description = "User to unblock"] user: User,
) -> Result<(), Error> {
    let author_id = ctx.author().id.get();
    let _ = get_db_user!(ctx, author_id);

    let db = &ctx.data().database;
    let removed = db.remove_blocked_user(author_id, user.id.get()).await?;

    let embed = if removed {
        CreateEmbed::new()
            .title(format!("{} User Unblocked", emojis::WIFI))
            .description(format!("<@{}> is no longer blocked.", user.id.get()))
            .color(Colour::DARK_GREEN)
    } else {
        CreateEmbed::new()
            .title(format!("{} No Change", emojis::WARNING))
            .description(format!("<@{}> was not on your block list.", user.id.get()))
            .color(Colour::ORANGE)
    };

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}
