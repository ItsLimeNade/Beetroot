use crate::data::{Context, Error};
use crate::utils::emojis;
use poise::serenity_prelude::{Colour, CreateEmbed};

pub const CURRENT_VERSION: &str = "1.2.0";

pub struct ChangelogEntry {
    pub version: &'static str,
    pub date: &'static str,
    pub changes: &'static [&'static str],
}

pub const CHANGELOG: &[ChangelogEntry] = &[
    ChangelogEntry {
        version: "1.2.0",
        date: "2026-06-15",
        changes: &[
            "Added `/tir` a Time in Range card over the last 7, 14, 30 or 90 days.",
            "Added `/theme` to build, import and apply custom color themes to your graphs.",
            "Your active theme now styles `/graph`, `/tir` and image-mode `/bg`.",
            "Added `/stickers` to view and remove the stickers on your graphs.",
            "Upgraded the rendering engine to bonbon 0.4.",
        ],
    },
    ChangelogEntry {
        version: "1.1.0",
        date: "2026-05-27",
        changes: &[
            "Removed the bundled web dashboard. A new standalone site is coming.",
            "Added `/settings` plus dedicated commands for every preference.",
            "All command responses now use the custom Beetroot emoji set.",
            "Added an in-bot tip system that hints at unused features.",
            "Default fingerprick expiry lowered to 30 minutes.",
            "Added `/delete-account` to wipe your stored data on request.",
        ],
    },
];

pub async fn post_command_hook(ctx: Context<'_>) {
    if let Err(e) = try_send_changelog(ctx).await {
        tracing::warn!("changelog: failed to send: {}", e);
    }
}

async fn try_send_changelog(ctx: Context<'_>) -> Result<(), Error> {
    let db = &ctx.data().database;
    let user_id = ctx.author().id.get();

    let user = match db.get_user(user_id).await? {
        Some(u) => u,
        None => return Ok(()),
    };

    let last_seen = user.last_seen_version.as_deref();
    if last_seen == Some(CURRENT_VERSION) {
        return Ok(());
    }

    let new_entries: Vec<&ChangelogEntry> = match last_seen {
        Some(seen) => CHANGELOG.iter().take_while(|e| e.version != seen).collect(),
        None => CHANGELOG.iter().collect(),
    };

    if new_entries.is_empty() {
        db.update_user_last_seen_version(user_id, CURRENT_VERSION).await?;
        return Ok(());
    }

    let embed = build_embed(&new_entries);
    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    db.update_user_last_seen_version(user_id, CURRENT_VERSION).await?;
    Ok(())
}

fn build_embed(entries: &[&ChangelogEntry]) -> CreateEmbed {
    let mut embed = CreateEmbed::new()
        .title(format!("{} What's New", emojis::CELEBRATION))
        .description(format!(
            "Beetroot has been updated to **{}**. Here's what's changed since you last used it:",
            CURRENT_VERSION
        ))
        .color(Colour::from_rgb(87, 189, 79));

    for entry in entries {
        let body = entry
            .changes
            .iter()
            .map(|c| format!("- {}", c))
            .collect::<Vec<_>>()
            .join("\n");
        embed = embed.field(format!("v{} ({})", entry.version, entry.date), body, false);
    }

    embed
}
