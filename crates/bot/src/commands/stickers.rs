use crate::data::{Context, Error};
use crate::utils::emojis;
use beetroot_core::models::{Sticker, StickerCategory};
use futures::StreamExt;
use macros::track_analytics;
use poise::serenity_prelude::{self as serenity, ComponentInteractionDataKind};
use serenity::{
    ButtonStyle, Colour, CreateActionRow, CreateButton, CreateEmbed, CreateEmbedFooter,
    CreateInteractionResponse, CreateInteractionResponseMessage, CreateSelectMenu,
    CreateSelectMenuKind, CreateSelectMenuOption,
};
use std::time::Duration;

/// Discord allows at most 25 options in a select menu.
const MAX_MENU_OPTIONS: usize = 25;

/// View and manage the stickers that appear on your graphs.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("stickers")]
pub async fn stickers(ctx: Context<'_>) -> Result<(), Error> {
    let db = &ctx.data().database;
    let user_id = ctx.author().id.get();

    let mut current = db.get_all_user_stickers(user_id).await?;

    if current.is_empty() {
        let embed = empty_embed();
        ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
            .await?;
        return Ok(());
    }

    let reply_handle = ctx
        .send(
            poise::CreateReply::default()
                .embed(build_embed(&current))
                .components(build_components(&current))
                .ephemeral(true),
        )
        .await?;

    let msg = reply_handle.message().await?;

    let mut collector = serenity::ComponentInteractionCollector::new(ctx.serenity_context().shard.clone())
        .message_id(msg.id)
        .author_id(ctx.author().id)
        .timeout(Duration::from_secs(120))
        .stream();

    while let Some(interaction) = collector.next().await {
        match interaction.data.custom_id.as_str() {
            "stickers_clear" => {
                db.clear_user_stickers(user_id).await?;
                current.clear();
                interaction
                    .create_response(
                        &ctx.serenity_context().http,
                        CreateInteractionResponse::UpdateMessage(
                            CreateInteractionResponseMessage::new()
                                .embed(
                                    CreateEmbed::new()
                                        .title(format!("{} Stickers Cleared", emojis::REMOVE_USER))
                                        .description("All of your stickers have been removed.")
                                        .color(Colour::DARK_RED),
                                )
                                .components(vec![]),
                        ),
                    )
                    .await?;
                return Ok(());
            }
            "stickers_remove" => {
                let ComponentInteractionDataKind::StringSelect { values } = &interaction.data.kind
                else {
                    continue;
                };
                let Some(sticker_id) = values.first().and_then(|v| v.parse::<i64>().ok()) else {
                    continue;
                };

                db.delete_user_sticker(user_id, sticker_id).await?;
                current.retain(|s| s.id != sticker_id);

                let response = if current.is_empty() {
                    CreateInteractionResponseMessage::new()
                        .embed(empty_embed())
                        .components(vec![])
                } else {
                    CreateInteractionResponseMessage::new()
                        .embed(build_embed(&current))
                        .components(build_components(&current))
                };

                interaction
                    .create_response(
                        &ctx.serenity_context().http,
                        CreateInteractionResponse::UpdateMessage(response),
                    )
                    .await?;

                if current.is_empty() {
                    return Ok(());
                }
            }
            _ => continue,
        }
    }

    reply_handle
        .edit(
            ctx,
            poise::CreateReply::default()
                .embed(build_embed(&current).footer(CreateEmbedFooter::new(
                    "Menu expired - run /stickers again to manage them.",
                )))
                .components(vec![]),
        )
        .await?;

    Ok(())
}

fn empty_embed() -> CreateEmbed {
    CreateEmbed::new()
        .title(format!("{} Your Stickers", emojis::STICKER_ADD))
        .description(
            "You don't have any stickers yet.\n\n\
             Add some with `/add-sticker` or by right-clicking a message → Apps → **Add Sticker**.",
        )
        .color(Colour::LIGHT_GREY)
}

/// Build the summary embed grouping stickers by category.
fn build_embed(stickers: &[Sticker]) -> CreateEmbed {
    let mut embed = CreateEmbed::new()
        .title(format!("{} Your Stickers", emojis::STICKER_ADD))
        .description(format!(
            "You have **{}** sticker{}. Use the menu below to remove one.",
            stickers.len(),
            if stickers.len() == 1 { "" } else { "s" }
        ))
        .color(Colour::from_rgb(87, 189, 79));

    // First sticker image as a thumbnail for a bit of visual flair.
    if let Some(first) = stickers.first() {
        embed = embed.thumbnail(&first.sticker_url);
    }

    for category in StickerCategory::all_variants() {
        let in_cat: Vec<&Sticker> = stickers.iter().filter(|s| s.category == *category).collect();
        if in_cat.is_empty() {
            continue;
        }
        let body = in_cat
            .iter()
            .map(|s| format!("- {} `#{}`", display_name(s), s.id))
            .collect::<Vec<_>>()
            .join("\n");
        embed = embed.field(
            format!(
                "{} ({}/{})",
                category.display_name(),
                in_cat.len(),
                category.max_count()
            ),
            body,
            false,
        );
    }

    embed.footer(CreateEmbedFooter::new(
        "Removing a sticker is permanent. The menu stays open for 2 minutes.",
    ))
}

/// Build the remove select-menu plus a clear-all button.
fn build_components(stickers: &[Sticker]) -> Vec<CreateActionRow> {
    let options: Vec<CreateSelectMenuOption> = stickers
        .iter()
        .take(MAX_MENU_OPTIONS)
        .map(|s| {
            CreateSelectMenuOption::new(
                format!("[{}] {}", s.category.display_name(), display_name(s)),
                s.id.to_string(),
            )
        })
        .collect();

    let menu = CreateSelectMenu::new("stickers_remove", CreateSelectMenuKind::String { options })
        .placeholder("Remove a sticker…");

    vec![
        CreateActionRow::SelectMenu(menu),
        CreateActionRow::Buttons(vec![
            CreateButton::new("stickers_clear")
                .label("Clear all")
                .style(ButtonStyle::Danger),
        ]),
    ]
}

fn display_name(sticker: &Sticker) -> String {
    sticker
        .display_name
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Sticker".to_string())
}
