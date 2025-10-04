use crate::Handler;
use serenity::all::{
    Colour, CommandInteraction, Context, CreateEmbed, CreateInteractionResponse,
    CreateInteractionResponseMessage, InteractionContext,
};
use serenity::builder::CreateCommand;
use serenity::model::application::CommandType;

pub async fn run(
    handler: &Handler,
    context: &Context,
    interaction: &CommandInteraction,
) -> anyhow::Result<()> {
    // Get target message from context menu
    let resolved = &interaction.data.resolved;
    let target_message = if let Some(message) = resolved.messages.values().next() {
        message
    } else {
        crate::commands::error::run(
            context,
            interaction,
            "No message found in context menu interaction.",
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

    // Check sticker count limit (max 3)
    let sticker_count = handler.database.get_user_sticker_count(user_id).await?;
    if sticker_count >= 3 {
        crate::commands::error::run(
            context,
            interaction,
            "You already have the maximum number of stickers (3). Use `/sticker` to remove one first.",
        )
        .await?;
        return Ok(());
    }

    // Extract sticker from Discord message
    let sticker_info = if let Some(sticker) = target_message.sticker_items.first() {
        // Discord sticker found
        (
            sticker.name.clone(),
            format!("https://media.discordapp.net/stickers/{}.png", sticker.id),
        )
    } else if let Some(content) = extract_sticker_name(&target_message.content) {
        // Fallback to content-based extraction
        (content.clone(), format!("images/stickers/{}.png", content))
    } else {
        crate::commands::error::run(
            context,
            interaction,
            "This message doesn't contain a Discord sticker. Please right-click on a message with a sticker.",
        )
        .await?;
        return Ok(());
    };

    let (sticker_name, sticker_url) = sticker_info;

    // Generate random position and rotation
    let x_position: f32 = rand::random::<f32>() * 0.6 + 0.2; // 0.2 to 0.8
    let y_position: f32 = rand::random::<f32>() * 0.6 + 0.2; // 0.2 to 0.8
    let rotation: f32 = rand::random::<f32>() * 60.0 - 30.0; // -30.0 to 30.0

    // Use the sticker URL (for Discord stickers) or path (for local stickers)
    let sticker_path = sticker_url;

    // Insert sticker into database
    match handler
        .database
        .insert_sticker(
            user_id,
            &sticker_path,
            &sticker_name,
            x_position,
            y_position,
            rotation,
        )
        .await
    {
        Ok(_) => {
            let embed = CreateEmbed::new()
                .title("Sticker Added")
                .description(format!(
                    "Successfully added **{}** to your graph!\n\nIt will appear on your next `/graph` command.",
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
            tracing::error!("[STICKER] Failed to add sticker: {}", e);

            let embed = CreateEmbed::new()
                .title("Error")
                .description("Failed to add sticker. Please try again.")
                .color(Colour::RED);

            let response = CreateInteractionResponseMessage::new()
                .embed(embed)
                .ephemeral(true);

            interaction
                .create_response(&context.http, CreateInteractionResponse::Message(response))
                .await?;
        }
    }

    Ok(())
}

fn extract_sticker_name(content: &str) -> Option<String> {
    // Simple extraction - look for common sticker patterns
    // This is a basic implementation - you might want to make this more sophisticated
    let words: Vec<&str> = content.split_whitespace().collect();

    // Look for words that might be sticker names (alphanumeric, underscore)
    for word in &words {
        let clean_word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
        if clean_word.len() >= 3 && clean_word.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Some(clean_word.to_lowercase());
        }
    }

    // If no good word found, use first word
    if let Some(first_word) = words.first() {
        let clean_word = first_word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
        if !clean_word.is_empty() {
            return Some(clean_word.to_lowercase());
        }
    }

    None
}

pub fn register() -> CreateCommand {
    CreateCommand::new("Add Sticker")
        .kind(CommandType::Message)
        .contexts(vec![
            InteractionContext::Guild,
            InteractionContext::PrivateChannel,
        ])
}
