use crate::data::{Context, Error};
use crate::utils::emojis;
use poise::serenity_prelude::{Colour, CreateEmbed};
use rand::random_range;

pub struct InteractionAcked;

pub struct Tip {
    pub id: &'static str,
    pub text: &'static str,
}

pub const TIPS: &[Tip] = &[
    Tip {
        id: "fingerprick_expiry",
        text: "Use `/fingerprick-expiry` to set how long fingerprick readings stay attached to `/bg`.",
    },
    Tip {
        id: "image_mode",
        text: "Use `/image-mode` to get `/bg` as a clean card image instead of an embed.",
    },
    Tip {
        id: "allow",
        text: "Use `/allow` to share your data with friends while keeping your profile Private.",
    },
    Tip {
        id: "block",
        text: "Use `/block` to stop a user from interacting with your data.",
    },
    Tip {
        id: "privacy",
        text: "Use `/privacy` to switch between Private and Public visibility.",
    },
    Tip {
        id: "ephemeral",
        text: "Use `/ephemeral` to make every bot reply visible only to you.",
    },
    Tip {
        id: "microbolus",
        text: "Use `/microbolus` to set the threshold or hide microboluses from your graph.",
    },
    Tip {
        id: "graph_at",
        text: "Pass the `at` option to `/graph` to time-travel and view a past window (e.g. `2h`, `1h30m`).",
    },
    Tip {
        id: "bg_at",
        text: "Pass the `at` option to `/bg` to see what your glucose was earlier (e.g. `30m` ago).",
    },
    Tip {
        id: "graph_user",
        text: "Pass the `user` option to `/graph` or `/bg` to view another user's data if they allow it.",
    },
    Tip {
        id: "add_sticker",
        text: "Use `/add-sticker` to add reaction stickers that show on your graph based on glucose state.",
    },
    Tip {
        id: "add_sticker_menu",
        text: "Right-click any message and pick `Add Sticker` to save its image as a sticker instantly.",
    },
    Tip {
        id: "a1c",
        text: "Use `/a1c` to estimate your A1C from the last 90 days of glucose readings.",
    },
    Tip {
        id: "nutrition",
        text: "Use `/nutrition` to look up calories, carbs, and macros for any food.",
    },
    Tip {
        id: "settings",
        text: "Use `/settings` to see every setting and its current value in one place.",
    },
    Tip {
        id: "token",
        text: "Use `/token` to securely replace or remove your stored Nightscout token.",
    },
    Tip {
        id: "url",
        text: "Use `/url` to update your Nightscout URL without re-running the full `/setup`.",
    },
    Tip {
        id: "delete_account",
        text: "Use `/delete-account` if you ever want to wipe all of your stored data.",
    },
    Tip {
        id: "info",
        text: "Use `/info` to find the source code and a link to support the project.",
    },
    Tip {
        id: "changelog",
        text: "Use `/changelog` to browse what's new, page back through previous releases too.",
    },
    Tip {
        id: "encryption",
        text: "Your Nightscout token is encrypted with AES-256-GCM before it ever touches the database.",
    },
    Tip {
        id: "open_source",
        text: "Beetroot is fully open source. Check `/info` for a link to the repository.",
    },
    Tip {
        id: "donate",
        text: "Enjoying Beetroot? `/info` has a donation link that helps keep it running.",
    },
];

const SKIP_FOR_COMMANDS: &[&str] = &["setup", "token"];

pub async fn pre_command_hook(ctx: Context<'_>) {
    if SKIP_FOR_COMMANDS.contains(&ctx.command().name.as_str()) {
        return;
    }
    if let Err(e) = try_send_tip(ctx).await {
        tracing::warn!("tip: failed to send: {}", e);
    }
}

async fn try_send_tip(ctx: Context<'_>) -> Result<(), Error> {
    let db = &ctx.data().database;
    let user_id = ctx.author().id.get();

    let count = db.increment_command_count(user_id).await?;
    if count % 10 != 0 {
        return Ok(());
    }

    let seen = db.get_seen_tip_ids(user_id).await?;
    let unseen: Vec<&Tip> = TIPS
        .iter()
        .filter(|t| !seen.iter().any(|s| s == t.id))
        .collect();
    if unseen.is_empty() {
        return Ok(());
    }

    let idx = random_range(0..unseen.len());
    let tip = unseen[idx];

    let embed = CreateEmbed::new()
        .title(format!("{} Tip", emojis::TIP))
        .description(tip.text)
        .color(Colour::BLURPLE);

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    ctx.set_invocation_data(InteractionAcked).await;
    db.mark_tip_seen(user_id, tip.id).await?;
    Ok(())
}

pub async fn was_interaction_acked(ctx: Context<'_>) -> bool {
    ctx.invocation_data::<InteractionAcked>().await.is_some()
}

pub async fn safe_defer(ctx: Context<'_>) -> Result<(), Error> {
    if was_interaction_acked(ctx).await {
        return Ok(());
    }
    ctx.defer().await?;
    Ok(())
}

pub async fn safe_defer_ephemeral(ctx: Context<'_>) -> Result<(), Error> {
    if was_interaction_acked(ctx).await {
        return Ok(());
    }
    ctx.defer_ephemeral().await?;
    Ok(())
}
