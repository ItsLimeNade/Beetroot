use crate::data::{Context, Error};
use crate::utils::emojis;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateEmbed};

/// Set how long a fingerprick reading stays attached to /bg results.
#[poise::command(
    slash_command,
    rename = "fingerprick-expiry",
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("fingerprick_expiry")]
pub async fn fingerprick_expiry(
    ctx: Context<'_>,
    #[description = "How many minutes a fingerprick value stays valid (1 to 720)"]
    #[min = 1]
    #[max = 720]
    minutes: i64,
) -> Result<(), Error> {
    let user_id = ctx.author().id.get();
    let _ = get_db_user!(ctx, user_id);

    let clamped = minutes.clamp(1, 720);
    if clamped != minutes {
        tracing::debug!(
            requested = minutes,
            clamped,
            "fingerprick expiry out of range, clamped"
        );
    }

    let db = &ctx.data().database;
    db.set_mbg_expiry_time(user_id, clamped).await?;
    tracing::info!(
        user = %crate::logging::redact(user_id),
        minutes = clamped,
        "fingerprick expiry updated"
    );

    let embed = CreateEmbed::new()
        .title(format!("{} Fingerprick Expiry Updated", emojis::water()))
        .description(format!(
            "Fingerprick readings will be shown for **{} minutes** after they are recorded.",
            clamped
        ))
        .color(Colour::DARK_GREEN);

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}
