use crate::bot::Handler;
use serenity::all::{
    CommandInteraction, CommandOptionType, Context, CreateInteractionResponse,
    CreateInteractionResponseMessage, InteractionContext, ResolvedOption, ResolvedValue,
};
use serenity::builder::{CreateCommand, CreateCommandOption};

pub async fn run(
    handler: &Handler,
    context: &Context,
    interaction: &CommandInteraction,
) -> anyhow::Result<()> {
    let mut sticker_id: Option<String> = None;

    for option in &interaction.data.options() {
        if let ResolvedOption {
            name: "sticker-id",
            value: ResolvedValue::String(id),
            ..
        } = option
        {
            sticker_id = Some(id.to_string());
            break;
        }
    }

    let sticker_id = if let Some(id) = sticker_id {
        id
    } else {
        crate::commands::error::run(
            context,
            interaction,
            "Please provide a sticker ID to remove.",
        )
        .await?;
        return Ok(());
    };

    let user_id = interaction.user.id.get();

    // Check if user exists in database
    if !handler.database.user_exists(user_id).await? {
        crate::commands::error::run(
            context,
            interaction,
            "You need to register your Nightscout URL first. Use `/setup` to get started.",
        )
        .await?;
        return Ok(());
    }

    // Create the file path from the sticker ID
    let sticker_path = format!("images/stickers/{}.webp", sticker_id);

    // Try to remove the sticker
    match handler
        .database
        .delete_sticker_by_name(user_id, &sticker_path)
        .await
    {
        Ok(_) => {
            let response = CreateInteractionResponseMessage::new()
                .content(format!(
                    "✅ Removed sticker \"{}\" from your graph!",
                    sticker_id
                ))
                .ephemeral(true);

            interaction
                .create_response(&context.http, CreateInteractionResponse::Message(response))
                .await?;
        }
        Err(e) => {
            tracing::error!("[STICKER] Failed to remove sticker: {}", e);
            crate::commands::error::run(
                context,
                interaction,
                &format!(
                    "Failed to remove sticker \"{}\". Make sure you have a sticker with that ID.",
                    sticker_id
                ),
            )
            .await?;
        }
    }

    Ok(())
}

pub fn register() -> CreateCommand {
    CreateCommand::new("remove-sticker")
        .description("Remove a sticker from your graph.")
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::String,
                "sticker-id",
                "The ID (name) of the sticker to remove (e.g., 'cute_cat')",
            )
            .required(true),
        )
        .contexts(vec![
            InteractionContext::Guild,
            InteractionContext::PrivateChannel,
        ])
}