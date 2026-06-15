use crate::data::{Context, Error};
use crate::utils::emojis;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateEmbed};

const FATSECRET_LOGO: &str =
    "https://platform.fatsecret.com/api/static/images/powered_by_fatsecret_square_brand.png";

#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("info")]
/// About Beetroot: links, credits, and how to support the project.
pub async fn info(ctx: Context<'_>) -> Result<(), Error> {
    let embed = CreateEmbed::new()
        .title("Beetroot")
        .url("https://github.com/ItsLimeNade/Beetroot")
        .color(Colour::from_rgb(87, 189, 79))
        .description(
            "A free, open-source Discord bot for blood glucose monitoring via Nightscout. \
             Check your readings, view graphs, look up nutrition info and more, right from Discord.\n\n\
             Beetroot is completely free to use. If it's been helpful, a small donation goes a long way \
             toward keeping it running!",
        )
        .field(
            format!("{} Support", emojis::KOFI_LOGO),
            "[Ko-fi](https://ko-fi.com/limenade)",
            false,
        )
        .field(
            format!("{} Source Code", emojis::BUG_GREEN),
            "[Beetroot](https://github.com/ItsLimeNade/Beetroot)",
            false,
        )
        .field(
            format!("{} Nutrition data", emojis::NUTRITION),
            "[Powered by FatSecret Platform API](https://platform.fatsecret.com)",
            false,
        )
        .image(FATSECRET_LOGO);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}
