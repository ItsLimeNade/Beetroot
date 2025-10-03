use crate::Handler;
use serenity::all::{
    ButtonStyle, Colour, CommandInteraction, ComponentInteraction, Context, CreateActionRow,
    CreateButton, CreateEmbed, CreateInteractionResponse, CreateInteractionResponseMessage,
    InteractionContext,
};
use serenity::builder::CreateCommand;

pub async fn run(
    handler: &Handler,
    context: &Context,
    interaction: &CommandInteraction,
) -> anyhow::Result<()> {
    let user_id = interaction.user.id.get();

    if !handler.database.user_exists(user_id).await? {
        crate::commands::error::run(
            context,
            interaction,
            "You need to register your Nightscout URL first. Use `/setup` to get started.",
        )
        .await?;
        return Ok(());
    }

    let stickers = handler.database.get_user_stickers(user_id).await?;

    if stickers.is_empty() {
        let embed = CreateEmbed::new()
            .title("Your Stickers")
            .description("You don't have any stickers yet!\n\nUse the **\"Add Sticker\"** context menu on a message with a sticker to add one.")
            .color(Colour::ORANGE);

        let response = CreateInteractionResponseMessage::new()
            .embed(embed)
            .ephemeral(true);

        interaction
            .create_response(&context.http, CreateInteractionResponse::Message(response))
            .await?;
        return Ok(());
    }

    let mut buttons = Vec::new();
    for (_index, sticker) in stickers.iter().enumerate().take(3) {
        let button = CreateButton::new(format!("remove_sticker_{}", sticker.id))
            .label(format!("Remove {}", sticker.display_name))
            .style(ButtonStyle::Danger);
        buttons.push(button);
    }

    if !stickers.is_empty() {
        let clear_all_button = CreateButton::new("clear_all_stickers")
            .label("Clear All")
            .style(ButtonStyle::Secondary);
        buttons.push(clear_all_button);
    }

    let action_row = CreateActionRow::Buttons(buttons);

    let sticker_list: String = stickers
        .iter()
        .map(|sticker| format!("• {}", sticker.display_name))
        .collect::<Vec<String>>()
        .join("\n");

    let embed = CreateEmbed::new()
        .title("Your Stickers")
        .description(format!(
            "**{}/3 stickers:**\n{}",
            stickers.len(),
            sticker_list
        ))
        .field("Info", "To add a sticker to your graph, use the context menu command `Applications > Add Sticker` when right clicking a sticker sent in chat.", true)
        .color(Colour::BLUE)
        .footer(serenity::all::CreateEmbedFooter::new(
            "Click a button below to remove a sticker",
        ));

    let response = CreateInteractionResponseMessage::new()
        .embed(embed)
        .components(vec![action_row])
        .ephemeral(true);

    interaction
        .create_response(&context.http, CreateInteractionResponse::Message(response))
        .await?;

    Ok(())
}

pub async fn handle_button(
    handler: &Handler,
    context: &Context,
    interaction: &ComponentInteraction,
) -> anyhow::Result<()> {
    let custom_id = &interaction.data.custom_id;

    if custom_id == "clear_all_stickers" {
        let user_id = interaction.user.id.get();

        match handler.database.clear_user_stickers(user_id).await {
            Ok(_) => {
                let embed = CreateEmbed::new()
                    .title("All Stickers Cleared")
                    .description("Successfully removed all stickers from your graph!")
                    .color(Colour::ORANGE);

                let response = CreateInteractionResponseMessage::new()
                    .embed(embed)
                    .ephemeral(true);

                interaction
                    .create_response(&context.http, CreateInteractionResponse::Message(response))
                    .await?;
            }
            Err(e) => {
                tracing::error!("[STICKER] Failed to clear all stickers: {}", e);

                let embed = CreateEmbed::new()
                    .title("Error")
                    .description("Failed to clear all stickers. Please try again.")
                    .color(Colour::RED);

                let response = CreateInteractionResponseMessage::new()
                    .embed(embed)
                    .ephemeral(true);

                interaction
                    .create_response(&context.http, CreateInteractionResponse::Message(response))
                    .await?;
            }
        }
    } else if let Some(sticker_id_str) = custom_id.strip_prefix("remove_sticker_") {
        let sticker_id: i32 = sticker_id_str.parse()?;
        let user_id = interaction.user.id.get();

        let stickers = handler.database.get_user_stickers(user_id).await?;
        let sticker_name = stickers
            .iter()
            .find(|s| s.id == sticker_id)
            .map(|s| s.display_name.clone())
            .unwrap_or_else(|| "Unknown".to_string());

        match handler.database.delete_sticker(sticker_id).await {
            Ok(_) => {
                let embed = CreateEmbed::new()
                    .title("Sticker Removed")
                    .description(format!(
                        "Successfully removed **{}** from your graph!",
                        sticker_name
                    ))
                    .color(Colour::DARK_GREEN);

                let response = CreateInteractionResponseMessage::new()
                    .embed(embed)
                    .ephemeral(true);

                interaction
                    .create_response(&context.http, CreateInteractionResponse::Message(response))
                    .await?;
            }
            Err(e) => {
                tracing::error!("[STICKER] Failed to remove sticker: {}", e);

                let embed = CreateEmbed::new()
                    .title("Error")
                    .description(format!(
                        "Failed to remove sticker **{}**. Please try again.",
                        sticker_name
                    ))
                    .color(Colour::RED);

                let response = CreateInteractionResponseMessage::new()
                    .embed(embed)
                    .ephemeral(true);

                interaction
                    .create_response(&context.http, CreateInteractionResponse::Message(response))
                    .await?;
            }
        }
    }

    Ok(())
}

pub fn register() -> CreateCommand {
    CreateCommand::new("stickers")
        .description("Manage your stickers - view and remove stickers from your graph.")
        .contexts(vec![
            InteractionContext::Guild,
            InteractionContext::PrivateChannel,
        ])
}
