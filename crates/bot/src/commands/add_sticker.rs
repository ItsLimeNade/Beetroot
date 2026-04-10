use crate::data::{Context, Error};
use crate::stickers::overlay::validate_image_url;
use database::models::StickerCategory;
use macros::track_analytics;
use poise::serenity_prelude::{self as serenity, ComponentInteractionCollector};
use serenity::{
    ButtonStyle, Colour, CreateActionRow, CreateButton, CreateEmbed, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};


/// Add a sticker to your graph by providing an image URL.
#[poise::command(
    slash_command,
    rename = "add-sticker",
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("add_sticker")]
pub async fn add_sticker(
    ctx: Context<'_>,
    #[description = "Direct URL to the sticker image (png, jpg, webp, gif)"] url: String,
    #[description = "Which glucose state triggers this sticker"]
    #[rename = "category"]
    category_choice: StickerCategoryChoice,
) -> Result<(), Error> {
    let db = &ctx.data().database;
    let user_id = ctx.author().id.get();

    get_db_user!(ctx, user_id);

    let category = category_choice.into_model();

    // Check category limit
    let count = db.get_sticker_count_by_category(user_id, category).await?;
    if count >= category.max_count() {
        send_error!(
            ctx,
            "Category Full",
            format!(
                "You already have {}/{} **{}** stickers. Remove one first with `/stickers`.",
                count,
                category.max_count(),
                category.display_name()
            )
        );
        return Ok(());
    }

    if db.sticker_url_exists(user_id, &url).await? {
        send_error!(
            ctx,
            "Duplicate",
            "You already have a sticker with this URL."
        );
        return Ok(());
    }

    ctx.defer_ephemeral().await?;

    if let Err(e) = validate_image_url(&url).await {
        send_error!(
            ctx,
            "Invalid Image",
            format!(
                "The URL doesn't point to a valid image: {}\n\n\
                Make sure the link goes directly to an image file.",
                e
            )
        );
        return Ok(());
    }

    let display_name = extract_display_name(&url);

    db.insert_sticker(user_id, &url, &display_name, category)
        .await?;

    let embed = CreateEmbed::new()
        .title("✅ Sticker Added")
        .description(format!(
            "**{}** added to your **{}** stickers!\n\n\
            It will appear on your next `/graph` when your glucose is {}.",
            display_name,
            category.display_name(),
            category_condition_text(category),
        ))
        .thumbnail(&url)
        .color(Colour::DARK_GREEN);

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}


#[poise::command(
    context_menu_command = "Add Sticker",
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
pub async fn add_sticker_context(
    ctx: Context<'_>,
    #[description = "Message to extract sticker from"] message: serenity::Message,
) -> Result<(), Error> {
    let db = &ctx.data().database;
    let user_id = ctx.author().id.get();

    get_db_user!(ctx, user_id);

    let (sticker_url, sticker_name) = match extract_sticker_from_message(&message) {
        Ok(result) => result,
        Err(e) => {
            send_error!(ctx, "No Sticker Found", e.to_string());
            return Ok(());
        }
    };

    if db.sticker_url_exists(user_id, &sticker_url).await? {
        send_error!(
            ctx,
            "Duplicate",
            "You already have a sticker with this URL."
        );
        return Ok(());
    }

    let buttons = vec![
        CreateButton::new("sticker_cat_low")
            .label(format!("Low ({} max)", StickerCategory::Low.max_count()))
            .style(ButtonStyle::Danger),
        CreateButton::new("sticker_cat_inrange")
            .label(format!(
                "In Range ({} max)",
                StickerCategory::InRange.max_count()
            ))
            .style(ButtonStyle::Success),
        CreateButton::new("sticker_cat_high")
            .label(format!("High ({} max)", StickerCategory::High.max_count()))
            .style(ButtonStyle::Primary),
        CreateButton::new("sticker_cat_other")
            .label(format!("Any ({} max)", StickerCategory::Other.max_count()))
            .style(ButtonStyle::Secondary),
    ];

    let embed = CreateEmbed::new()
        .title("Select Sticker Category")
        .description(format!(
            "Choose a category for **{}**:\n\n\
            🔴 **Low** — Appears when glucose is below target\n\
            🟢 **In Range** — Appears when glucose is in range\n\
            🟠 **High** — Appears when glucose is above target\n\
            ⚪ **Any** — Appears regardless of glucose state",
            sticker_name
        ))
        .thumbnail(&sticker_url)
        .color(Colour::BLURPLE);

    let reply_handle = ctx
        .send(
            poise::CreateReply::default()
                .embed(embed)
                .components(vec![CreateActionRow::Buttons(buttons)])
                .ephemeral(true),
        )
        .await?;

    let msg = reply_handle.message().await?;

    let interaction = ComponentInteractionCollector::new(ctx.serenity_context().shard.clone())
        .message_id(msg.id)
        .author_id(ctx.author().id)
        .timeout(std::time::Duration::from_secs(30))
        .await;

    let Some(interaction) = interaction else {
        let expired_embed = CreateEmbed::new()
            .title("⏰ Timed Out")
            .description("Category selection expired. Use the command again to add a sticker.")
            .color(Colour::LIGHT_GREY);

        reply_handle
            .edit(
                ctx,
                poise::CreateReply::default()
                    .embed(expired_embed)
                    .components(vec![]),
            )
            .await?;
        return Ok(());
    };

    let category = match interaction.data.custom_id.as_str() {
        "sticker_cat_low" => StickerCategory::Low,
        "sticker_cat_inrange" => StickerCategory::InRange,
        "sticker_cat_high" => StickerCategory::High,
        "sticker_cat_other" => StickerCategory::Other,
        _ => return Ok(()),
    };

    let count = db.get_sticker_count_by_category(user_id, category).await?;
    if count >= category.max_count() {
        let embed = CreateEmbed::new()
            .title("❌ Category Full")
            .description(format!(
                "You already have {}/{} **{}** stickers.\n\
                Use `/stickers` to remove one first.",
                count,
                category.max_count(),
                category.display_name()
            ))
            .color(Colour::RED);

        interaction
            .create_response(
                &ctx.serenity_context().http,
                CreateInteractionResponse::UpdateMessage(
                    CreateInteractionResponseMessage::new()
                        .embed(embed)
                        .components(vec![]),
                ),
            )
            .await?;
        return Ok(());
    }

    db.insert_sticker(user_id, &sticker_url, &sticker_name, category)
        .await?;

    let embed = CreateEmbed::new()
        .title("✅ Sticker Added")
        .description(format!(
            "**{}** added to your **{}** stickers!\n\n\
            It will appear on your next `/graph` when your glucose is {}.",
            sticker_name,
            category.display_name(),
            category_condition_text(category),
        ))
        .thumbnail(&sticker_url)
        .color(Colour::DARK_GREEN);

    interaction
        .create_response(
            &ctx.serenity_context().http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .embed(embed)
                    .components(vec![]),
            ),
        )
        .await?;

    Ok(())
}


#[derive(Debug, poise::ChoiceParameter)]
pub enum StickerCategoryChoice {
    #[name = "Low (glucose below target)"]
    Low,
    #[name = "In Range"]
    InRange,
    #[name = "High (glucose above target)"]
    High,
    #[name = "Any / No Context"]
    Other,
}

impl StickerCategoryChoice {
    fn into_model(self) -> StickerCategory {
        match self {
            Self::Low => StickerCategory::Low,
            Self::InRange => StickerCategory::InRange,
            Self::High => StickerCategory::High,
            Self::Other => StickerCategory::Other,
        }
    }
}

fn extract_sticker_from_message(message: &serenity::Message) -> Result<(String, String), Error> {
    if let Some(sticker) = message.sticker_items.first() {
        let url = format!(
            "https://media.discordapp.net/stickers/{}.png?size=320",
            sticker.id
        );
        return Ok((url, sticker.name.clone()));
    }

    if let Some(attachment) = message.attachments.first() {
        if is_image_content_type(attachment.content_type.as_deref()) {
            return Ok((attachment.url.clone(), attachment.filename.clone()));
        }
    }

    if let Some(embed) = message.embeds.first() {
        if let Some(ref img) = embed.image {
            return Ok((img.url.clone(), "Embedded Image".to_string()));
        }
        if let Some(ref thumb) = embed.thumbnail {
            return Ok((thumb.url.clone(), "Embedded Thumbnail".to_string()));
        }
    }

    Err(anyhow::anyhow!(
        "This message doesn't contain a sticker, image, or embed.\n\n\
        Try right-clicking a message that has a Discord sticker or an attached image."
    ))
}

fn is_image_content_type(ct: Option<&str>) -> bool {
    ct.is_some_and(|s| s.starts_with("image/"))
}

fn extract_display_name(url: &str) -> String {
    url.rsplit('/')
        .next()
        .and_then(|filename| filename.split('?').next())
        .map(|name| {
            name.trim_end_matches(".png")
                .trim_end_matches(".jpg")
                .trim_end_matches(".jpeg")
                .trim_end_matches(".webp")
                .trim_end_matches(".gif")
                .replace(['_', '-'], " ")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Custom Sticker".to_string())
}

fn category_condition_text(category: StickerCategory) -> &'static str {
    match category {
        StickerCategory::Low => "below target",
        StickerCategory::InRange => "in range",
        StickerCategory::High => "above target",
        StickerCategory::Other => "in any state",
    }
}
