use crate::data::{Context, Error};
use crate::utils::emojis;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateEmbed};

/// Configure microbolus threshold and whether they show on graphs.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("microbolus")]
pub async fn microbolus(
    ctx: Context<'_>,
    #[description = "Bolus size in units below which it counts as a microbolus"]
    #[min = 0.0]
    #[max = 50.0]
    threshold: Option<f64>,
    #[description = "Show microboluses on your graphs"] display: Option<bool>,
) -> Result<(), Error> {
    let user_id = ctx.author().id.get();
    let _ = get_db_user!(ctx, user_id);

    if threshold.is_none() && display.is_none() {
        tracing::debug!(user = %crate::logging::redact(user_id), "microbolus called with no fields");
        send_error!(
            ctx,
            "Nothing to Update",
            "Provide a threshold, a display toggle, or both."
        );
        return Ok(());
    }

    let db = &ctx.data().database;
    let mut lines = Vec::new();

    if let Some(t) = threshold {
        let clamped = t.clamp(0.0, 50.0);
        db.set_microbolus_threshold(user_id, clamped).await?;
        tracing::info!(
            user = %crate::logging::redact(user_id),
            threshold = clamped,
            "microbolus threshold updated"
        );
        lines.push(format!("Threshold set to **{} U**.", clamped));
    }

    if let Some(d) = display {
        db.set_display_microbolus(user_id, d).await?;
        tracing::info!(
            user = %crate::logging::redact(user_id),
            display = d,
            "microbolus display updated"
        );
        let label = if d { "shown" } else { "hidden" };
        lines.push(format!(
            "Microboluses will be **{}** on your graphs.",
            label
        ));
    }

    let embed = CreateEmbed::new()
        .title(format!(
            "{} Microbolus Settings Updated",
            emojis::MICRO_BOLUS
        ))
        .description(lines.join("\n"))
        .color(Colour::DARK_GREEN);

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}
