use crate::Handler;
use crate::utils::database::StickerCategory;
use serenity::all::{
    ButtonStyle, Colour, CommandInteraction, ComponentInteraction, Context, CreateActionRow,
    CreateButton, CreateEmbed, CreateInteractionResponse, CreateInteractionResponseMessage,
    InteractionContext,
};
use serenity::builder::CreateCommand;
use serenity::model::application::CommandType;

pub async fn run(
    handler: &Handler,
    context: &Context,
    interaction: &CommandInteraction,
) -> anyhow::Result<()> {
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

    if !handler.database.user_exists(user_id).await? {
        crate::commands::error::run(
            context,
            interaction,
            "You need to register your Nightscout URL first. Use `/setup` to get started.",
        )
        .await?;
        return Ok(());
    }

    let sticker_info = if let Some(sticker) = target_message.sticker_items.first() {
        (
            sticker.name.clone(),
            format!("https://media.discordapp.net/stickers/{}.png", sticker.id),
        )
    } else if let Some(content) = extract_sticker_name(&target_message.content) {
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

    let buttons = vec![
        CreateButton::new(format!("add_sticker_low:{}:{}", sticker_name, sticker_url))
            .label("Low (3 max)")
            .style(ButtonStyle::Danger),
        CreateButton::new(format!(
            "add_sticker_inrange:{}:{}",
            sticker_name, sticker_url
        ))
        .label("In Range (3 max)")
        .style(ButtonStyle::Success),
        CreateButton::new(format!("add_sticker_high:{}:{}", sticker_name, sticker_url))
            .label("High (3 max)")
            .style(ButtonStyle::Primary),
        CreateButton::new(format!("add_sticker_any:{}:{}", sticker_name, sticker_url))
            .label("Any (5 max)")
            .style(ButtonStyle::Secondary),
    ];

    let action_row = CreateActionRow::Buttons(buttons);

    let embed = CreateEmbed::new()
        .title("Select Sticker Category")
        .description(format!(
            "Choose a category for **{}**:\n\n\
            • **Low**: Shows when blood glucose is low (<70 mg/dL)\n\
            • **In Range**: Shows when blood glucose is in range (70-180 mg/dL)\n\
            • **High**: Shows when blood glucose is high (>180 mg/dL)\n\
            • **Any**: Shows randomly regardless of blood glucose",
            sticker_name
        ))
        .color(Colour::BLUE);

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

    if let Some(data) = custom_id.strip_prefix("add_sticker_") {
        let parts: Vec<&str> = data.splitn(3, ':').collect();
        if parts.len() != 3 {
            return Ok(());
        }

        let category_str = parts[0];
        let sticker_name = parts[1];
        let sticker_url = parts[2];

        let category = match category_str {
            "low" => StickerCategory::Low,
            "inrange" => StickerCategory::InRange,
            "high" => StickerCategory::High,
            "any" => StickerCategory::Any,
            _ => return Ok(()),
        };

        let user_id = interaction.user.id.get();

        let sticker_count = handler
            .database
            .get_user_sticker_count_by_category(user_id, category)
            .await?;

        if sticker_count >= category.max_count() {
            let embed = CreateEmbed::new()
                .title("Category Full")
                .description(format!(
                    "You already have the maximum number of **{}** stickers ({}).\n\
                    Use `/stickers category:{}` to remove one first.",
                    category.display_name(),
                    category.max_count(),
                    category.display_name()
                ))
                .color(Colour::RED);

            let response = CreateInteractionResponseMessage::new()
                .embed(embed)
                .ephemeral(true);

            interaction
                .create_response(&context.http, CreateInteractionResponse::Message(response))
                .await?;
            return Ok(());
        }

        match handler
            .database
            .insert_sticker(user_id, sticker_url, sticker_name, category)
            .await
        {
            Ok(_) => {
                let embed = CreateEmbed::new()
                    .title("Sticker Added")
                    .description(format!(
                        "Successfully added **{}** to your **{}** stickers!\n\n\
                        It will appear on your next `/graph` command when your blood glucose is {}.",
                        sticker_name,
                        category.display_name(),
                        match category {
                            StickerCategory::Low => "low (<70 mg/dL)",
                            StickerCategory::InRange => "in range (70-180 mg/dL)",
                            StickerCategory::High => "high (>180 mg/dL)",
                            StickerCategory::Any => "in any state",
                        }
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
    }

    Ok(())
}

fn extract_sticker_name(content: &str) -> Option<String> {
    let words: Vec<&str> = content.split_whitespace().collect();

    for word in &words {
        let clean_word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
        if clean_word.len() >= 3 && clean_word.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Some(clean_word.to_lowercase());
        }
    }

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
