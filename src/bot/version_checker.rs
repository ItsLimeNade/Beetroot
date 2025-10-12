use crate::bot::Handler;
use crate::commands;
use anyhow::Result;
use serenity::all::{CommandInteraction, Context, CreateInteractionResponseFollowup};

/// Check if user needs to be notified of version update and send notification
pub async fn check_and_notify_version_update(
    handler: &Handler,
    context: &Context,
    command: &CommandInteraction,
) -> Result<()> {
    let current_version = dotenvy::var("BOT_VERSION").unwrap_or_else(|_| "0.1.1".to_string());
    let user_id = command.user.id.get();

    match handler.database.get_user_last_seen_version(user_id).await {
        Ok(last_seen_version) => {
            if last_seen_version != current_version {
                let embed = commands::update_message::create_update_embed(&current_version);
                let response = CreateInteractionResponseFollowup::new()
                    .embed(embed)
                    .ephemeral(true);

                if let Err(e) = command.create_followup(&context.http, response).await {
                    tracing::warn!("[VERSION] Failed to send update notification: {}", e);
                }

                if let Err(e) = handler
                    .database
                    .update_user_last_seen_version(user_id, &current_version)
                    .await
                {
                    tracing::error!("[VERSION] Failed to update last seen version: {}", e);
                } else {
                    tracing::info!(
                        "[VERSION] User {} notified of update from {} to {}",
                        user_id,
                        last_seen_version,
                        current_version
                    );
                }
            }
        }
        Err(e) => {
            tracing::debug!(
                "[VERSION] Could not get last seen version for user {}: {}",
                user_id,
                e
            );
        }
    }

    Ok(())
}
