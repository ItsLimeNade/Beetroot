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

            //TODO Make a better error embed later.
            let _ = ctx
                .send(
                    poise::CreateReply::default()
                        .content("An unexpected error occurred. Please try again later.")
                        .ephemeral(true),
                )
                .await;
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
    _ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    _data: &Data,
) -> Result<(), Error> {
    if let serenity::FullEvent::Ready { data_about_bot, .. } = event {
        tracing::info!(
            bot = %data_about_bot.user.name,
            guilds = data_about_bot.guilds.len(),
            "gateway ready"
        );
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
