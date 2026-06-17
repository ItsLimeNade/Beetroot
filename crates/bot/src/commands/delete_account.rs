use crate::data::{Context, Error};
use crate::utils::emojis;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{
    ButtonStyle, Colour, ComponentInteractionCollector, CreateActionRow, CreateButton, CreateEmbed,
    CreateInteractionResponse, CreateInteractionResponseMessage,
};

/// Permanently delete your stored Beetroot data.
#[poise::command(
    slash_command,
    rename = "delete-account",
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("delete_account")]
pub async fn delete_account(ctx: Context<'_>) -> Result<(), Error> {
    let user_id = ctx.author().id.get();
    let db = &ctx.data().database;

    if !db.user_exists(user_id).await? {
        send_error!(
            ctx,
            "Nothing to Delete",
            "You do not have a stored account."
        );
        return Ok(());
    }

    let buttons = CreateActionRow::Buttons(vec![
        CreateButton::new("delete_confirm")
            .label("Delete everything")
            .style(ButtonStyle::Danger),
        CreateButton::new("delete_cancel")
            .label("Cancel")
            .style(ButtonStyle::Secondary),
    ]);

    let warning = CreateEmbed::new()
        .title(format!("{} Delete Account", emojis::WARNING))
        .description(
            "This will permanently remove your Nightscout configuration, settings, and all of your stickers.\n\nThis action cannot be undone.",
        )
        .color(Colour::RED);

    let reply = ctx
        .send(
            poise::CreateReply::default()
                .embed(warning)
                .components(vec![buttons])
                .ephemeral(true),
        )
        .await?;

    let msg = reply.message().await?;

    let interaction = ComponentInteractionCollector::new(ctx.serenity_context())
        .message_id(msg.id)
        .author_id(ctx.author().id)
        .timeout(std::time::Duration::from_secs(30))
        .await;

    let Some(mci) = interaction else {
        let timed_out = CreateEmbed::new()
            .title(format!("{} Timed Out", emojis::SYNC_PROBLEM))
            .description("Confirmation expired. Run the command again if you still want to delete your account.")
            .color(Colour::LIGHT_GREY);
        reply
            .edit(
                ctx,
                poise::CreateReply::default()
                    .embed(timed_out)
                    .components(vec![]),
            )
            .await?;
        return Ok(());
    };

    match mci.data.custom_id.as_str() {
        "delete_confirm" => {
            tracing::info!(user = %crate::logging::redact(user_id), "deleting account (confirmed)");
            db.delete_user(user_id).await?;
            tracing::info!(user = %crate::logging::redact(user_id), "account deleted");
            let done = CreateEmbed::new()
                .title(format!("{} Account Deleted", emojis::REMOVE_USER))
                .description("Your data has been removed. Run /setup any time to start again.")
                .color(Colour::DARK_GREEN);
            mci.create_response(
                ctx.serenity_context(),
                CreateInteractionResponse::UpdateMessage(
                    CreateInteractionResponseMessage::new()
                        .embed(done)
                        .components(vec![]),
                ),
            )
            .await?;
        }
        _ => {
            let cancelled = CreateEmbed::new()
                .title(format!("{} Cancelled", emojis::WIFI))
                .description("Your account is untouched.")
                .color(Colour::LIGHT_GREY);
            mci.create_response(
                ctx.serenity_context(),
                CreateInteractionResponse::UpdateMessage(
                    CreateInteractionResponseMessage::new()
                        .embed(cancelled)
                        .components(vec![]),
                ),
            )
            .await?;
        }
    }

    Ok(())
}
