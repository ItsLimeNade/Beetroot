use crate::data::{Context, Error};
use crate::utils::emojis;
use beetroot_core::db::TokenUpdate;
use macros::track_analytics;
use poise::Modal;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateEmbed};

#[derive(Debug, poise::ChoiceParameter)]
pub enum TokenAction {
    #[name = "Replace"]
    Replace,
    #[name = "Remove"]
    Remove,
}

#[derive(Debug, Modal)]
#[name = "Nightscout Token"]
struct TokenModal {
    #[name = "New Nightscout Token"]
    #[placeholder = "Paste the new token here"]
    token: String,
}

/// Update or remove your stored Nightscout token.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("token")]
pub async fn token(
    ctx: Context<'_>,
    #[description = "Replace your token with a new one, or remove it"] action: TokenAction,
) -> Result<(), Error> {
    let user_id = ctx.author().id.get();
    let _ = get_db_user!(ctx, user_id);

    let db = &ctx.data().database;

    match action {
        TokenAction::Remove => {
            db.set_nightscout_token(user_id, TokenUpdate::Clear).await?;
            // Never log token values, even with LOG_SENSITIVE: they are credentials.
            tracing::info!(user = %crate::logging::redact(user_id), "nightscout token cleared");
            let embed = CreateEmbed::new()
                .title(format!("{} Token Removed", emojis::lock_open()))
                .description("Your Nightscout token has been cleared.")
                .color(Colour::DARK_GREEN);
            ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
                .await?;
        }
        TokenAction::Replace => {
            let app_ctx = match ctx {
                poise::Context::Application(c) => c,
                _ => return Ok(()),
            };

            let data = match poise::execute_modal::<_, _, TokenModal>(app_ctx, None, None).await? {
                Some(d) => d,
                None => return Ok(()),
            };

            let trimmed = data.token.trim();
            if trimmed.is_empty() {
                tracing::debug!(user = %crate::logging::redact(user_id), "rejected empty token");
                send_error!(ctx, "Empty Token", "The new token cannot be empty.");
                return Ok(());
            }

            db.set_nightscout_token(user_id, TokenUpdate::Set(trimmed))
                .await?;
            // Log only that it changed, never the token value.
            tracing::info!(user = %crate::logging::redact(user_id), "nightscout token replaced");

            let embed = CreateEmbed::new()
                .title(format!("{} Token Updated", emojis::password()))
                .description("Your Nightscout token has been replaced and re-encrypted.")
                .color(Colour::DARK_GREEN);

            ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
                .await?;
        }
    }

    Ok(())
}
