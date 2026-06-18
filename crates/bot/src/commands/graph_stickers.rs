use crate::data::{Context, Error};
use crate::utils::emojis;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateEmbed};

/// Most stickers we will scatter on a graph, for now.
pub const MAX_GRAPH_STICKERS: i64 = 30;

/// Set how many stickers appear on your /graph.
#[poise::command(
    slash_command,
    rename = "graph-stickers",
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("graph_stickers")]
pub async fn graph_stickers(
    ctx: Context<'_>,
    #[description = "How many stickers to scatter on your graph (0 to 30)"]
    #[min = 0]
    #[max = 30]
    count: i64,
) -> Result<(), Error> {
    let user_id = ctx.author().id.get();
    let _ = get_db_user!(ctx, user_id);

    let clamped = count.clamp(0, MAX_GRAPH_STICKERS);
    let db = &ctx.data().database;
    db.set_graph_sticker_count(user_id, clamped).await?;
    tracing::info!(
        user = %crate::logging::redact(user_id),
        count = clamped,
        "graph sticker count updated"
    );

    let body = if clamped == 0 {
        "Stickers are now hidden on your graph.".to_string()
    } else {
        format!(
            "Up to **{}** stickers will be scattered on your graph (you need stickers added with `/add-sticker`).",
            clamped
        )
    };

    let embed = CreateEmbed::new()
        .title(format!("{} Graph Stickers", emojis::sticker_add()))
        .description(body)
        .color(Colour::DARK_GREEN);

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}
