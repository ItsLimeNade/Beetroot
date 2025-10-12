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

    tracing::info!(
        "[VERSION] Checking version for user {}. Current version: {}",
        user_id,
        current_version
    );

    match handler.database.get_user_last_seen_version(user_id).await {
        Ok(last_seen_version) => {
            tracing::info!(
                "[VERSION] User {} last seen version: {}",
                user_id,
                last_seen_version
            );

            if last_seen_version != current_version {
                tracing::info!(
                    "[VERSION] Version mismatch. Sending update notification to user {}",
                    user_id
                );

                let embed = commands::update_message::create_update_embed(&current_version);
                let response = CreateInteractionResponseFollowup::new()
                    .embed(embed)
                    .ephemeral(true);

                if let Err(e) = command.create_followup(&context.http, response).await {
                    tracing::warn!(
                        "[VERSION] Failed to send update notification to user {}: {}",
                        user_id,
                        e
                    );
                } else {
                    tracing::info!(
                        "[VERSION] Successfully sent update notification to user {}",
                        user_id
                    );
                }

                if let Err(e) = handler
                    .database
                    .update_user_last_seen_version(user_id, &current_version)
                    .await
                {
                    tracing::error!("[VERSION] Failed to update last seen version: {}", e);
                } else {
                    tracing::info!(
                        "[VERSION] User {} version updated from {} to {}",
                        user_id,
                        last_seen_version,
                        current_version
                    );
                }
            } else {
                tracing::debug!(
                    "[VERSION] User {} already on current version {}",
                    user_id,
                    current_version
                );
            }
        }
        Err(_) => {
            tracing::info!(
                "[VERSION] No last seen version for user {}. Sending initial notification",
                user_id
            );

            let embed = commands::update_message::create_update_embed(&current_version);
            let response = CreateInteractionResponseFollowup::new()
                .embed(embed)
                .ephemeral(true);

            if let Err(e) = command.create_followup(&context.http, response).await {
                tracing::warn!(
                    "[VERSION] Failed to send initial version notification to user {}: {}",
                    user_id,
                    e
                );
            } else {
                tracing::info!(
                    "[VERSION] Successfully sent initial notification to user {}",
                    user_id
                );
            }

            if let Err(e) = handler
                .database
                .update_user_last_seen_version(user_id, &current_version)
                .await
            {
                tracing::error!("[VERSION] Failed to set initial version: {}", e);
            } else {
                tracing::info!(
                    "[VERSION] User {} initial version set to {}",
                    user_id,
                    current_version
                );
            }
        }
    }

    Ok(())
}
