use crate::data::{Context, Data, Error};
use poise::serenity_prelude as serenity;

/// Centralized error handler.
/// This is called whenever a command returns an Err or panics.
pub async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    match error {
        poise::FrameworkError::Setup { error, .. } => {
            panic!("Failed to start bot: {:?}", error);
        }
        poise::FrameworkError::Command { error, ctx, .. } => {
            tracing::error!(
                cmd = %ctx.command().qualified_name,
                user = %crate::logging::redact(ctx.author().id.get()),
                error = ?error,
                "command failed"
            );

            send_error!(
                ctx,
                "Something Went Wrong",
                "An unexpected error occurred. Please try again later."
            );
        }
        poise::FrameworkError::CooldownHit {
            remaining_cooldown,
            ctx,
            ..
        } => {
            let secs = remaining_cooldown.as_secs().max(1);
            send_error!(
                ctx,
                "Slow Down",
                format!(
                    "That command is on cooldown. Try again in {secs} second{}.",
                    if secs == 1 { "" } else { "s" }
                )
            );
        }
        error => {
            if let Err(e) = poise::builtins::on_error(error).await {
                tracing::error!("Error while handling error: {}", e);
            }
        }
    }
}

/// Generic event handler for raw Discord events
pub async fn event_handler(
    ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    _data: &Data,
) -> Result<(), Error> {
    match event {
        serenity::FullEvent::Ready { data_about_bot, .. } => {
            tracing::info!(
                bot = %data_about_bot.user.name,
                guilds = data_about_bot.guilds.len(),
                "gateway ready"
            );
        }
        serenity::FullEvent::InteractionCreate {
            interaction: serenity::Interaction::Component(mci),
            ..
        } => {
            crate::commands::nutrition::handle_foreign_component(ctx, mci).await;
        }
        _ => {}
    }
    Ok(())
}

/// Called before every command. Logs the invocation, then runs the tip hook.
pub async fn pre_command(ctx: Context<'_>) {
    tracing::info!(
        cmd = %ctx.command().qualified_name,
        user = %crate::logging::redact(ctx.author().id.get()),
        "command invoked"
    );

    crate::tips::pre_command_hook(ctx).await;
}

/// Called after every successful command execution.
pub async fn post_command(ctx: Context<'_>) {
    crate::changelog::post_command_hook(ctx).await;

    tracing::debug!(
        cmd = %ctx.command().qualified_name,
        user = %crate::logging::redact(ctx.author().id.get()),
        "command completed"
    );
}
