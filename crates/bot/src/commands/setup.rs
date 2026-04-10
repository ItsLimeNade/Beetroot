use crate::data::{Context, Error};
use poise::Modal;
use poise::serenity_prelude as serenity;
use serenity::{
    ButtonStyle, Colour, CreateActionRow, CreateButton, CreateEmbed, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};
use url::{ParseError, Url};

#[derive(Debug, Modal)]
#[name = "Nightscout Setup"]
struct SetupModal {
    #[name = "Nightscout URL"]
    #[placeholder = "https://your-site.herokuapp.com"]
    nightscout_url: String,

    #[name = "Nightscout Token (optional)"]
    #[placeholder = "Leave empty if your site is public"]
    nightscout_token: Option<String>,
}

/// Setup your Nightscout URL and privacy settings
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
pub async fn setup(ctx: Context<'_>) -> Result<(), Error> {
    let app_ctx = match ctx {
        poise::Context::Application(c) => c,
        _ => return Ok(()),
    };

    let modal_data = match poise::execute_modal::<_, _, SetupModal>(app_ctx, None, None).await? {
        Some(data) => data,
        None => return Ok(()),
    };

    let url_obj = match parse_and_normalize_url(&modal_data.nightscout_url) {
        Ok(u) => u,
        Err(e) => {
            send_error!(ctx, "Invalid URL", e);
            return Ok(());
        }
    };

    let url_str = url_obj.to_string();

    ctx.defer_ephemeral().await?;

    verify_nightscout_connection!(ctx, &url_str, modal_data.nightscout_token.clone());

    show_privacy_selection(ctx, url_str, modal_data.nightscout_token).await?;

    Ok(())
}

async fn show_privacy_selection(
    ctx: Context<'_>,
    url: String,
    token: Option<String>,
) -> Result<(), Error> {
    let buttons = CreateActionRow::Buttons(vec![
        CreateButton::new("setup_public")
            .label("Public")
            .style(ButtonStyle::Success),
        CreateButton::new("setup_private")
            .label("Private")
            .style(ButtonStyle::Secondary),
    ]);

    let token_text = if token.is_some() {
        "\n\n🔐 **Access Token:** Securely Encrypted"
    } else {
        "\n\n🔓 **No Token:** Public Access"
    };

    let embed = CreateEmbed::new()
        .title("Privacy Settings")
        .description(format!(
            "Connection successful! Who can see data from **{}**?\n\n**Public:** Anyone via commands\n**Private:** Only you (and allowed users){}", 
            url, token_text
        ))
        .color(Colour::BLURPLE);

    let reply = ctx
        .send(
            poise::CreateReply::default()
                .embed(embed)
                .components(vec![buttons])
                .ephemeral(true),
        )
        .await?;

    let msg = reply.message().await?;

    if let Some(mci) = serenity::ComponentInteractionCollector::new(ctx.serenity_context())
        .message_id(msg.id)
        .timeout(std::time::Duration::from_secs(60))
        .author_id(ctx.author().id)
        .await
    {
        let is_private = match mci.data.custom_id.as_str() {
            "setup_private" => true,
            "setup_public" => false,
            _ => return Ok(()),
        };

        // Database Update
        let database = &ctx.data().database;
        let update_result = database
            .update_user_nightscout(ctx.author().id.get(), &url, token.as_deref(), is_private)
            .await;

        match update_result {
            Ok(_) => {
                let privacy_text = if is_private { "Private" } else { "Public" };
                let success_embed = CreateEmbed::new()
                    .title("Setup Complete")
                    .description(format!(
                        "✅ Nightscout configured successfully!\n\n**URL:** {}\n**Privacy:** {}",
                        url, privacy_text
                    ))
                    .color(Colour::DARK_GREEN);

                mci.create_response(
                    ctx.serenity_context(),
                    CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .embed(success_embed)
                            .components(vec![]),
                    ),
                )
                .await?;
            }
            Err(e) => {
                tracing::error!("Database error: {}", e);
                mci.create_response(
                    ctx.serenity_context(),
                    CreateInteractionResponse::Message(
                        serenity::CreateInteractionResponseMessage::new()
                            .content("❌ Database error. Please try again.")
                            .ephemeral(true),
                    ),
                )
                .await?;
            }
        }
    } else {
        reply
            .edit(
                ctx,
                poise::CreateReply::default()
                    .embed(
                        CreateEmbed::new()
                            .title("Timed Out")
                            .description("Setup timed out. Please run `/setup` again.")
                            .color(Colour::RED),
                    )
                    .components(vec![]),
            )
            .await?;
    }

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
