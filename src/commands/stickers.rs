use crate::bot::Handler;
use crate::utils::database::StickerCategory;
use serenity::all::{
    ButtonStyle, Colour, CommandInteraction, CommandOptionType, ComponentInteraction, Context,
    CreateActionRow, CreateButton, CreateCommand, CreateCommandOption, CreateEmbed,
    CreateInteractionResponse, CreateInteractionResponseMessage, InteractionContext,
    ResolvedOption, ResolvedValue,
};

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

    let options = interaction.data.options();
    let category_filter = if let Some(ResolvedOption {
        value: ResolvedValue::String(cat_str),
        ..
    }) = options.first()
    {
        cat_str
    } else {
        "All"
    };

    if category_filter.to_lowercase() == "all" {
        show_all_stickers_paginated(handler, context, interaction, 0).await?;
    } else if let Some(category) = StickerCategory::from_str(category_filter) {
        show_category_stickers(handler, context, interaction, category).await?;
    } else {
        crate::commands::error::run(
            context,
            interaction,
            "Invalid category. Please choose: Low, In Range, High, Any, or All.",
        )
        .await?;
    }

    Ok(())
}

async fn show_category_stickers(
    handler: &Handler,
    context: &Context,
    interaction: &CommandInteraction,
    category: StickerCategory,
) -> anyhow::Result<()> {
    let user_id = interaction.user.id.get();
    let stickers = handler
        .database
        .get_user_stickers_by_category(user_id, category)
        .await?;

    if stickers.is_empty() {
        let embed = CreateEmbed::new()
            .title(format!("{} Stickers", category.display_name()))
            .description(format!(
                "You don't have any **{}** stickers yet!\n\n\
                Use the **\"Add Sticker\"** context menu on a message with a sticker to add one.",
                category.display_name()
            ))
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
    for sticker in stickers.iter().take(3) {
        let button = CreateButton::new(format!("remove_sticker_{}", sticker.id))
            .label(format!("Remove {}", sticker.display_name))
            .style(ButtonStyle::Danger);
        buttons.push(button);
    }

    if !stickers.is_empty() {
        let clear_button =
            CreateButton::new(format!("clear_category_stickers_{}", category.to_str()))
                .label(format!("Clear All {}", category.display_name()))
                .style(ButtonStyle::Secondary);
        buttons.push(clear_button);
    }

    let action_row = CreateActionRow::Buttons(buttons);

    let sticker_list: String = stickers
        .iter()
        .map(|sticker| format!("• {}", sticker.display_name))
        .collect::<Vec<String>>()
        .join("\n");

    let embed = CreateEmbed::new()
        .title(format!("{} Stickers", category.display_name()))
        .description(format!(
            "**{}/{} stickers:**\n{}",
            stickers.len(),
            category.max_count(),
            sticker_list
        ))
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

async fn show_all_stickers_paginated(
    handler: &Handler,
    context: &Context,
    interaction: &CommandInteraction,
    page: usize,
) -> anyhow::Result<()> {
    let user_id = interaction.user.id.get();
    let all_stickers = handler.database.get_user_stickers(user_id).await?;

    if all_stickers.is_empty() {
        let embed = CreateEmbed::new()
            .title("Your Stickers")
            .description(
                "You don't have any stickers yet!\n\n\
                Use the **\"Add Sticker\"** context menu on a message with a sticker to add one.",
            )
            .color(Colour::ORANGE);

        let response = CreateInteractionResponseMessage::new()
            .embed(embed)
            .ephemeral(true);

        interaction
            .create_response(&context.http, CreateInteractionResponse::Message(response))
            .await?;
        return Ok(());
    }

    let mut categorized: std::collections::HashMap<StickerCategory, Vec<_>> =
        std::collections::HashMap::new();
    for sticker in &all_stickers {
        categorized
            .entry(sticker.category)
            .or_insert_with(Vec::new)
            .push(sticker);
    }

    let stickers_per_page = 3;
    let total_pages = all_stickers.len().div_ceil(stickers_per_page);
    let page = page.min(total_pages.saturating_sub(1));

    let start_idx = page * stickers_per_page;
    let end_idx = (start_idx + stickers_per_page).min(all_stickers.len());
    let page_stickers = &all_stickers[start_idx..end_idx];

    let mut buttons = Vec::new();
    for sticker in page_stickers {
        let button = CreateButton::new(format!("remove_sticker_{}", sticker.id))
            .label(format!("Remove {}", sticker.display_name))
            .style(ButtonStyle::Danger);
        buttons.push(button);
    }

    let mut action_rows = vec![CreateActionRow::Buttons(buttons)];

    if total_pages > 1 {
        let mut nav_buttons = Vec::new();

        if page > 0 {
            nav_buttons.push(
                CreateButton::new(format!("stickers_page_{}", page - 1))
                    .label("◀ Previous")
                    .style(ButtonStyle::Primary),
            );
        }

        nav_buttons.push(
            CreateButton::new("stickers_page_info")
                .label(format!("Page {}/{}", page + 1, total_pages))
                .style(ButtonStyle::Secondary)
                .disabled(true),
        );

        if page < total_pages - 1 {
            nav_buttons.push(
                CreateButton::new(format!("stickers_page_{}", page + 1))
                    .label("Next ▶")
                    .style(ButtonStyle::Primary),
            );
        }

        action_rows.push(CreateActionRow::Buttons(nav_buttons));
    }

    let mut description = String::from("**Your stickers by category:**\n\n");

    for category in &[
        StickerCategory::Low,
        StickerCategory::InRange,
        StickerCategory::High,
        StickerCategory::Any,
    ] {
        let count = categorized.get(category).map_or(0, |v| v.len());
        description.push_str(&format!(
            "**{}**: {}/{}\n",
            category.display_name(),
            count,
            category.max_count()
        ));
    }

    description.push_str("\n**Stickers on this page:**\n");
    for sticker in page_stickers {
        description.push_str(&format!(
            "• {} ({})\n",
            sticker.display_name,
            sticker.category.display_name()
        ));
    }

    let embed = CreateEmbed::new()
        .title("All Stickers")
        .description(description)
        .color(Colour::BLUE)
        .footer(serenity::all::CreateEmbedFooter::new(
            "Click a button to remove a sticker, or use navigation to see more",
        ));

    let response = CreateInteractionResponseMessage::new()
        .embed(embed)
        .components(action_rows)
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

    if let Some(page_str) = custom_id.strip_prefix("stickers_page_")
        && let Ok(page) = page_str.parse::<usize>()
    {
        let user_id = interaction.user.id.get();
        let all_stickers = handler.database.get_user_stickers(user_id).await?;

        let stickers_per_page = 3;
        let total_pages = all_stickers.len().div_ceil(stickers_per_page);
        let page = page.min(total_pages.saturating_sub(1));

        let start_idx = page * stickers_per_page;
        let end_idx = (start_idx + stickers_per_page).min(all_stickers.len());
        let page_stickers = &all_stickers[start_idx..end_idx];

        let mut buttons = Vec::new();
        for sticker in page_stickers {
            let button = CreateButton::new(format!("remove_sticker_{}", sticker.id))
                .label(format!("Remove {}", sticker.display_name))
                .style(ButtonStyle::Danger);
            buttons.push(button);
        }

        let mut action_rows = vec![CreateActionRow::Buttons(buttons)];

        if total_pages > 1 {
            let mut nav_buttons = Vec::new();

            if page > 0 {
                nav_buttons.push(
                    CreateButton::new(format!("stickers_page_{}", page - 1))
                        .label("◀ Previous")
                        .style(ButtonStyle::Primary),
                );
            }

            nav_buttons.push(
                CreateButton::new("stickers_page_info")
                    .label(format!("Page {}/{}", page + 1, total_pages))
                    .style(ButtonStyle::Secondary)
                    .disabled(true),
            );

            if page < total_pages - 1 {
                nav_buttons.push(
                    CreateButton::new(format!("stickers_page_{}", page + 1))
                        .label("Next ▶")
                        .style(ButtonStyle::Primary),
                );
            }

            action_rows.push(CreateActionRow::Buttons(nav_buttons));
        }

        let mut categorized: std::collections::HashMap<StickerCategory, Vec<_>> =
            std::collections::HashMap::new();
        for sticker in &all_stickers {
            categorized
                .entry(sticker.category)
                .or_insert_with(Vec::new)
                .push(sticker);
        }

        let mut description = String::from("**Your stickers by category:**\n\n");
        for category in &[
            StickerCategory::Low,
            StickerCategory::InRange,
            StickerCategory::High,
            StickerCategory::Any,
        ] {
            let count = categorized.get(category).map_or(0, |v| v.len());
            description.push_str(&format!(
                "**{}**: {}/{}\n",
                category.display_name(),
                count,
                category.max_count()
            ));
        }

        description.push_str("\n**Stickers on this page:**\n");
        for sticker in page_stickers {
            description.push_str(&format!(
                "• {} ({})\n",
                sticker.display_name,
                sticker.category.display_name()
            ));
        }

        let embed = CreateEmbed::new()
            .title("All Stickers")
            .description(description)
            .color(Colour::BLUE)
            .footer(serenity::all::CreateEmbedFooter::new(
                "Click a button to remove a sticker, or use navigation to see more",
            ));

        let response = CreateInteractionResponseMessage::new()
            .embed(embed)
            .components(action_rows)
            .ephemeral(true);

        interaction
            .create_response(
                &context.http,
                CreateInteractionResponse::UpdateMessage(response),
            )
            .await?;
        return Ok(());
    }

    if let Some(category_str) = custom_id.strip_prefix("clear_category_stickers_")
        && let Some(category) = StickerCategory::from_str(category_str)
    {
        let user_id = interaction.user.id.get();
        let stickers = handler
            .database
            .get_user_stickers_by_category(user_id, category)
            .await?;

        for sticker in stickers {
            handler.database.delete_sticker(sticker.id).await?;
        }

        let embed = CreateEmbed::new()
            .title("Stickers Cleared")
            .description(format!(
                "Successfully removed all **{}** stickers!",
                category.display_name()
            ))
            .color(Colour::ORANGE);

        let response = CreateInteractionResponseMessage::new()
            .embed(embed)
            .ephemeral(true);

        interaction
            .create_response(&context.http, CreateInteractionResponse::Message(response))
            .await?;
        return Ok(());
    }

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
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::String,
                "category",
                "Filter by category (Low, In Range, High, Any, or All)",
            )
            .required(true)
            .add_string_choice("All", "All")
            .add_string_choice("Low", "Low")
            .add_string_choice("In Range", "In Range")
            .add_string_choice("High", "High")
            .add_string_choice("Any", "Any"),
        )
        .contexts(vec![
            InteractionContext::Guild,
            InteractionContext::PrivateChannel,
        ])
}
