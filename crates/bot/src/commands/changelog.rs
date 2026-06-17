use crate::changelog::{CHANGELOG, CURRENT_VERSION, ChangelogEntry};
use crate::data::{Context, Error};
use crate::utils::emojis;
use futures::StreamExt;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{
    ButtonStyle, Colour, ComponentInteractionCollector, CreateActionRow, CreateButton, CreateEmbed,
    CreateEmbedFooter, CreateInteractionResponse, CreateInteractionResponseMessage,
};
use std::time::Duration;

/// How long the navigation buttons stay live before they're stripped.
const BROWSE_TIMEOUT: Duration = Duration::from_secs(120);

/// Browse Beetroot's changelog, newest first.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("changelog")]
pub async fn changelog(ctx: Context<'_>) -> Result<(), Error> {
    if CHANGELOG.is_empty() {
        send_error!(ctx, "No Changelog", "There aren't any release notes yet.");
        return Ok(());
    }

    // Viewing the changelog counts as "seen", so the command popup
    // for the current version doesn't fire on top of this one.
    let user_id = ctx.author().id.get();
    let db = &ctx.data().database;
    if let Err(e) = db
        .update_user_last_seen_version(user_id, CURRENT_VERSION)
        .await
    {
        tracing::warn!(error = %e, "failed to update last-seen version");
    }

    let total = CHANGELOG.len();
    let mut index = 0usize; // 0 == newest

    if total == 1 {
        ctx.send(
            poise::CreateReply::default()
                .embed(build_page(index, total))
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    let reply = ctx
        .send(
            poise::CreateReply::default()
                .embed(build_page(index, total))
                .components(build_components(index, total))
                .ephemeral(true),
        )
        .await?;

    let msg = reply.message().await?;

    let mut collector = ComponentInteractionCollector::new(ctx.serenity_context())
        .message_id(msg.id)
        .author_id(ctx.author().id)
        .timeout(BROWSE_TIMEOUT)
        .stream();

    while let Some(mci) = collector.next().await {
        match mci.data.custom_id.as_str() {
            "changelog_newer" => index = index.saturating_sub(1),
            "changelog_older" => index = (index + 1).min(total - 1),
            _ => continue,
        }

        mci.create_response(
            ctx.serenity_context(),
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .embed(build_page(index, total))
                    .components(build_components(index, total)),
            ),
        )
        .await?;
    }

    reply
        .edit(
            ctx,
            poise::CreateReply::default()
                .embed(build_page(index, total))
                .components(vec![]),
        )
        .await?;

    Ok(())
}

/// Render a single changelog entry as an embed page.
fn build_page(index: usize, total: usize) -> CreateEmbed {
    let entry: &ChangelogEntry = &CHANGELOG[index];

    let body = entry
        .changes
        .iter()
        .map(|c| format!("- {}", c))
        .collect::<Vec<_>>()
        .join("\n");

    let latest_tag = if index == 0 { " • Latest" } else { "" };

    CreateEmbed::new()
        .title(format!("{} Changelog", emojis::CELEBRATION))
        .color(Colour::from_rgb(87, 189, 79))
        .field(format!("v{} ({})", entry.version, entry.date), body, false)
        .field(
            "Full changelog",
            format!(
                "[Read the full release notes on GitHub]({})",
                crate::changelog::RELEASES_URL
            ),
            false,
        )
        .footer(CreateEmbedFooter::new(format!(
            "Release {} of {}{}",
            index + 1,
            total,
            latest_tag
        )))
}

/// Build the Newer/Older navigation row
fn build_components(index: usize, total: usize) -> Vec<CreateActionRow> {
    vec![CreateActionRow::Buttons(vec![
        CreateButton::new("changelog_newer")
            .label("◀ Newer")
            .style(ButtonStyle::Secondary)
            .disabled(index == 0),
        CreateButton::new("changelog_older")
            .label("Older ▶")
            .style(ButtonStyle::Secondary)
            .disabled(index + 1 >= total),
    ])]
}
