use crate::data::{Context, Error};
use poise::Modal;
use poise::serenity_prelude as serenity;
use serenity::{
    ButtonStyle, Colour, CreateActionRow, CreateButton, CreateEmbed, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};
use url::Url;

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

/// Configure your Nightscout URL and privacy settings
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
        None => return Ok(()), // User cancelled
    };

    let validated_url = match validate_and_fix_url(&modal_data.nightscout_url) {
        Ok(url) => url,
        Err(e) => {
            ctx.send(
                poise::CreateReply::default()
                    .content(format!("❌ Invalid URL: {}", e))
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        }
    };

    ctx.defer_ephemeral().await?;

    let client_result = cinnamon::client::NightscoutClient::new(
        &validated_url,
        modal_data.nightscout_token.clone(),
    );

    let check_result = match client_result {
        Ok(client) => client
            .entries()
            .sgv()
            .list()
            .limit(1)
            .await
            .map_err(|e| anyhow::anyhow!(e)),
        Err(_) => Err(anyhow::anyhow!("Invalid URL format")),
    };

    match check_result {
        Ok(_) => {
            show_privacy_selection(ctx, &validated_url, modal_data.nightscout_token).await?;
        }
        Err(e) => {
            let error_embed = CreateEmbed::new()
                .title("Connection Failed")
                .description(format!(
                    "Could not connect to your Nightscout site.\n\n**Error:** `{}`\n\nPlease verify:\n• The URL is correct\n• Your site is online\n• The token is correct (if required)", 
                    e
                ))
                .color(Colour::RED);

            ctx.send(
                poise::CreateReply::default()
                    .embed(error_embed)
                    .ephemeral(true),
            )
            .await?;
        }
    }

    Ok(())
}

async fn show_privacy_selection(
    ctx: Context<'_>,
    url: &str,
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
        "\n\n🔐 **Access Token:** Will be saved securely (encrypted)"
    } else {
        "\n\n🔓 **No Token:** Requests will be made without authentication"
    };

    let embed = CreateEmbed::new()
        .title("Privacy Settings")
        .description(format!(
            "Connection successful! Choose who can see your blood glucose data for **{}**:\n\n**Public:** Anyone can use commands to see your data\n**Private:** Only you and people you specifically allow can see your data{}", 
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

        let database = &ctx.data().database;
        let update_result = database
            .update_user_nightscout(ctx.author().id.get(), url, token.as_deref(), is_private)
            .await;

        match update_result {
            Ok(_) => {
                let privacy_text = if is_private { "Private" } else { "Public" };
                let success_embed = CreateEmbed::new()
                    .title("Setup Complete")
                    .description(format!(
                        "✅ Your Nightscout has been configured!\n\n**URL:** {}\n**Privacy:** {}",
                        url, privacy_text
                    ))
                    .color(Colour::DARK_GREEN);

                let response = CreateInteractionResponseMessage::new()
                    .embed(success_embed)
                    .components(vec![]); // Remove buttons

                mci.create_response(
                    ctx.serenity_context(),
                    CreateInteractionResponse::UpdateMessage(response),
                )
                .await?;
            }
            Err(e) => {
                tracing::error!("Failed to save user data: {}", e);
                mci.create_response(
                    ctx.serenity_context(),
                    CreateInteractionResponse::Message(
                        serenity::CreateInteractionResponseMessage::new()
                            .content("Failed to save data to database. Please try again.")
                            .ephemeral(true),
                    ),
                )
                .await?;
            }
        }
    } else {
        let timeout_embed = CreateEmbed::new()
            .title("Setup Timed Out")
            .description("You didn't select a privacy option in time. Please run `/setup` again.")
            .color(Colour::RED);

        reply
            .edit(
                ctx,
                poise::CreateReply::default()
                    .embed(timeout_embed)
                    .components(vec![]),
            )
            .await?;
    }

    Ok(())
}

fn validate_and_fix_url(input: &str) -> Result<String, String> {
    let mut url = input.trim().to_string();

    if url.is_empty() {
        return Err("URL cannot be empty".to_string());
    }

    if !url.starts_with("http://") && !url.starts_with("https://") {
        url = format!("https://{}", url);
    }

    if !url.ends_with('/') {
        url.push('/');
    }

    match Url::parse(&url) {
        Ok(parsed) => {
            if parsed.host().is_none() {
                return Err("URL must have a valid domain name".to_string());
            }
            if parsed.scheme() != "http" && parsed.scheme() != "https" {
                return Err("URL must use http or https protocol".to_string());
            }
            Ok(url)
        }
        Err(e) => Err(format!("Invalid URL format: {}", e)),
    }
}
