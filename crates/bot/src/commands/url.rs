use crate::data::{Context, Error};
use crate::utils::emojis;
use crate::utils::net::parse_and_normalize_url;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateEmbed};

/// Update only your Nightscout URL without re-running /setup.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("url")]
pub async fn url(
    ctx: Context<'_>,
    #[description = "Your Nightscout URL (https://...)"] new_url: String,
) -> Result<(), Error> {
    let user_id = ctx.author().id.get();
    let user_data = get_db_user!(ctx, user_id);

    let parsed = match parse_and_normalize_url(&new_url) {
        Ok(u) => u,
        Err(e) => {
            send_error!(ctx, "Invalid URL", e);
            return Ok(());
        }
    };
    let url_str = parsed.to_string();

    crate::tips::safe_defer_ephemeral(ctx).await?;

    verify_nightscout_connection!(ctx, &url_str, user_data.nightscout_token.clone());

    let db = &ctx.data().database;
    db.set_nightscout_url(user_id, &url_str).await?;

    let embed = CreateEmbed::new()
        .title(format!("{} Nightscout URL Updated", emojis::WIFI_ADD))
        .description(format!("Your Nightscout URL is now:\n`{}`", url_str))
        .color(Colour::DARK_GREEN);

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}
