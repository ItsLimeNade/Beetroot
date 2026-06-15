use crate::data::{Context, Error};
use crate::utils::emojis;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateEmbed, User};

/// Allow another user to see your private Nightscout data.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("allow")]
pub async fn allow(
    ctx: Context<'_>,
    #[description = "User to grant access to"] user: User,
) -> Result<(), Error> {
    let author_id = ctx.author().id.get();
    let _ = get_db_user!(ctx, author_id);

    if user.id.get() == author_id {
        send_error!(ctx, "Invalid Target", "You cannot allow yourself.");
        return Ok(());
    }
    if user.bot {
        send_error!(ctx, "Invalid Target", "Bots cannot be added to your allow list.");
        return Ok(());
    }

    let db = &ctx.data().database;
    let added = db.add_allowed_user(author_id, user.id.get()).await?;

    let embed = if added {
        CreateEmbed::new()
            .title(format!("{} User Allowed", emojis::ADD_USER))
            .description(format!("<@{}> can now view your data.", user.id.get()))
            .color(Colour::DARK_GREEN)
    } else {
        CreateEmbed::new()
            .title(format!("{} No Change", emojis::WARNING))
            .description(format!("<@{}> was already on your allow list.", user.id.get()))
            .color(Colour::ORANGE)
    };

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}
