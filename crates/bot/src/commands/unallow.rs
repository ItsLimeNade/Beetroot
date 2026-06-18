use crate::data::{Context, Error};
use crate::utils::emojis;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateEmbed, User};

/// Remove a user from your allow list.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("unallow")]
pub async fn unallow(
    ctx: Context<'_>,
    #[description = "User to revoke access from"] user: User,
) -> Result<(), Error> {
    let author_id = ctx.author().id.get();
    let _ = get_db_user!(ctx, author_id);

    let db = &ctx.data().database;
    let removed = db.remove_allowed_user(author_id, user.id.get()).await?;
    tracing::info!(
        user = %crate::logging::redact(author_id),
        target = %crate::logging::redact(user.id.get()),
        changed = removed,
        "allow list: removed user"
    );

    let embed = if removed {
        CreateEmbed::new()
            .title(format!("{} User Removed", emojis::remove_user()))
            .description(format!(
                "<@{}> can no longer view your private data.",
                user.id.get()
            ))
            .color(Colour::DARK_GREEN)
    } else {
        CreateEmbed::new()
            .title(format!("{} No Change", emojis::warning()))
            .description(format!("<@{}> was not on your allow list.", user.id.get()))
            .color(Colour::ORANGE)
    };

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}
