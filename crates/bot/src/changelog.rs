use crate::data::{Context, Error};
use crate::utils::emojis;
use poise::serenity_prelude::{Colour, CreateEmbed};

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const RELEASES_URL: &str = "https://github.com/itslimenade/Beetroot/releases/latest";

pub struct ChangelogEntry {
    pub version: &'static str,
    pub date: &'static str,
    pub changes: &'static [&'static str],
}

pub const CHANGELOG: &[ChangelogEntry] = &[ChangelogEntry {
    version: CURRENT_VERSION,
    date: "2026-06-17",
    changes: &[
        "Track your glucose with `/bg`, with an optional clean image card.",
        "See trends with `/graph` and `/tir`, a Time in Range card over 7, 14, 30 or 90 days.",
        "Look back in time with the new `at` option on `/bg` and `/graph` (e.g. `2h`, `1d`, `1w`, `1h30m`).",
        "Estimate your `/a1c` from recent readings.",
        "Personalize your graphs with `/theme` color themes and your own stickers.",
        "Look up calories, carbs and macros for any food with `/nutrition`.",
        "Tune everything from `/settings`, plus dedicated commands: `/privacy`, `/allow`, `/block`, `/ephemeral`, `/image-mode`, `/microbolus` and `/fingerprick-expiry`.",
        "Manage your account with `/stickers` and `/delete-account`.",
    ],
}];

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
        db.update_user_last_seen_version(user_id, CURRENT_VERSION)
            .await?;
        return Ok(());
    }

    let embed = build_embed(&new_entries);
    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    db.update_user_last_seen_version(user_id, CURRENT_VERSION)
        .await?;
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

    embed.field(
        "Full changelog",
        format!("[Read the full release notes on GitHub]({RELEASES_URL})"),
        false,
    )
}
