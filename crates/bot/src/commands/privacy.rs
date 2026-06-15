use crate::data::{Context, Error};
use crate::utils::emojis;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateEmbed};

#[derive(Debug, poise::ChoiceParameter)]
pub enum PrivacyMode {
    #[name = "Private"]
    Private,
    #[name = "Public"]
    Public,
}

/// Set your data visibility to private or public.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("privacy")]
pub async fn privacy(
    ctx: Context<'_>,
    #[description = "Who can see your Nightscout data"] mode: PrivacyMode,
) -> Result<(), Error> {
    let user_id = ctx.author().id.get();
    let _ = get_db_user!(ctx, user_id);

    let is_private = matches!(mode, PrivacyMode::Private);
    let db = &ctx.data().database;
    db.set_privacy(user_id, is_private).await?;

    let (icon, label, description) = if is_private {
        (
            emojis::LOCK_CLOSED,
            "Private",
            "Only you and users on your allow list can view your data.",
        )
    } else {
        (
            emojis::LOCK_OPEN,
            "Public",
            "Anyone can view your data via the bot.",
        )
    };

    let embed = CreateEmbed::new()
        .title(format!("{} Privacy Updated", icon))
        .description(format!("**{}**\n{}", label, description))
        .color(Colour::DARK_GREEN);

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}
