use crate::data::{Context, Error};
use crate::utils::emojis;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateEmbed, User};

/// Block a user from interacting with your data.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("block")]
pub async fn block(
    ctx: Context<'_>,
    #[description = "User to block"] user: User,
) -> Result<(), Error> {
    let author_id = ctx.author().id.get();
    let _ = get_db_user!(ctx, author_id);

    if user.id.get() == author_id {
        send_error!(ctx, "Invalid Target", "You cannot block yourself.");
        return Ok(());
    }
    if user.bot {
        send_error!(ctx, "Invalid Target", "Bots cannot be blocked.");
        return Ok(());
    }

    let db = &ctx.data().database;
    let added = db.add_blocked_user(author_id, user.id.get()).await?;

    let embed = if added {
        CreateEmbed::new()
            .title(format!("{} User Blocked", emojis::WIFI_LOCKED))
            .description(format!(
                "<@{}> can no longer interact with your data.",
                user.id.get()
            ))
            .color(Colour::DARK_GREEN)
    } else {
        CreateEmbed::new()
            .title(format!("{} No Change", emojis::WARNING))
            .description(format!("<@{}> was already blocked.", user.id.get()))
            .color(Colour::ORANGE)
    };

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}
