use crate::data::{Data, Error, Context};
use poise::serenity_prelude as serenity;

/// Centralized error handler.
/// This is called whenever a command returns an Err or panics.
pub async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    match error {
        poise::FrameworkError::Setup { error, .. } => {
            panic!("Failed to start bot: {:?}", error);
        }
        poise::FrameworkError::Command { error, ctx, .. } => {
            tracing::error!("Error in command '{}': {:?}", ctx.command().name, error);
            
            //TODO Make a better error embed later.
            let _ = ctx.send(poise::CreateReply::default()
                .content("An unexpected error occurred. Please try again later.")
                .ephemeral(true)
            ).await;
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
    match event {
        serenity::FullEvent::Ready { data_about_bot, .. } => {
            tracing::info!("[BOT] {} is ready and connected!", data_about_bot.user.name);
        }
        _ => {}
    }
    Ok(())
}

/// Called after every successful command execution.
pub async fn post_command(ctx: Context<'_>) {
    // let database = &ctx.data().database;
    // if let Ok(exists) = database.user_exists(ctx.author().id.get()).await {
    //     if exists {
    //          crate::bot::version_checker::check(ctx).await;
    //     }
    // }
    
    tracing::debug!("Executed command {} by {}", ctx.command().name, ctx.author().name);
}