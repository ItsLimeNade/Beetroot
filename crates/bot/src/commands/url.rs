use crate::data::{Context, Error};
use crate::utils::emojis;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateEmbed};
use url::{ParseError, Url};

/// Update only your Nightscout URL without re-running /setup.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("url")]
pub async fn url(
    ctx: Context<'_>,
    #[description = "Your Nightscout URL (https://...)"] new_url: String,
) -> Result<(), Error> {
    let user_id = ctx.author().id.get();
    let user_data = get_db_user!(ctx, user_id);

    let parsed = match parse_and_normalize_url(&new_url) {
        Ok(u) => u,
        Err(e) => {
            send_error!(ctx, "Invalid URL", e);
            return Ok(());
        }
    };
    let url_str = parsed.to_string();

    crate::tips::safe_defer_ephemeral(ctx).await?;

    verify_nightscout_connection!(ctx, &url_str, user_data.nightscout_token.clone());

    let db = &ctx.data().database;
    db.set_nightscout_url(user_id, &url_str).await?;

    let embed = CreateEmbed::new()
        .title(format!("{} Nightscout URL Updated", emojis::WIFI_ADD))
        .description(format!("Your Nightscout URL is now:\n`{}`", url_str))
        .color(Colour::DARK_GREEN);

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}

fn parse_and_normalize_url(input: &str) -> Result<Url, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("URL cannot be empty".to_string());
    }

    let mut url = match Url::parse(input) {
        Ok(u) => u,
        Err(ParseError::RelativeUrlWithoutBase) => Url::parse(&format!("https://{}", input))
            .map_err(|_| "Invalid URL format".to_string())?,
        Err(e) => return Err(format!("Invalid URL: {}", e)),
    };

    let scheme = url.scheme();
    if scheme != "http" && scheme != "https" {
        return Err("URL must start with http:// or https://".to_string());
    }

    if url.host().is_none() {
        return Err("URL must have a valid domain name".to_string());
    }

    if !url.path().ends_with('/')
        && let Ok(mut segments) = url.path_segments_mut()
    {
        segments.pop_if_empty().push("");
    }

    Ok(url)
}
