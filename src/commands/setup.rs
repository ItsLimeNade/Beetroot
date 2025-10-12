use crate::bot::Handler;
use crate::utils::database::NightscoutInfo;
use serenity::all::{
    ButtonStyle, Colour, CommandInteraction, ComponentInteraction, Context, CreateActionRow,
    CreateButton, CreateCommand, CreateEmbed, CreateInputText, CreateInteractionResponse,
    CreateInteractionResponseMessage, CreateQuickModal, InteractionContext,
};
use url::Url;

pub async fn run(
    handler: &Handler,
    context: &Context,
    interaction: &CommandInteraction,
) -> anyhow::Result<()> {
    let modal = CreateQuickModal::new("Nightscout Setup")
        .timeout(std::time::Duration::from_secs(300))
        .short_field("Nightscout URL")
        .field(
            CreateInputText::new(
                serenity::all::InputTextStyle::Short,
                "Nightscout Token (optional)",
                "",
            )
            .required(false)
            .placeholder("leave empty if you don't want to answer"),
        );

    let response = interaction.quick_modal(context, modal).await?;

    if let Some(modal_response) = response {
        let url_input = &modal_response.inputs[0];
        let token_input = modal_response
            .inputs
            .get(1)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());

        let validated_url = match validate_and_fix_url(url_input) {
            Ok(url) => url,
            Err(e) => {
                let error_embed = CreateEmbed::new()
                    .title("Invalid URL")
                    .description(format!("Please check your URL: {}", e))
                    .color(Colour::RED);

                let error_response = CreateInteractionResponseMessage::new()
                    .embed(error_embed)
                    .ephemeral(true);

                modal_response
                    .interaction
                    .create_response(context, CreateInteractionResponse::Message(error_response))
                    .await?;
                return Ok(());
            }
        };

        tracing::info!(
            "[TEST] Testing Nightscout connection for URL: {}",
            validated_url
        );
        match handler
            .nightscout_client
            .get_entry(&validated_url, token_input)
            .await
        {
            Ok(_) => {
                tracing::info!("[OK] Nightscout connection test successful");
                // URL works, show privacy selection
                show_privacy_selection(
                    context,
                    &modal_response.interaction,
                    &validated_url,
                    token_input.map(|s| s.to_string()),
                )
                .await?;
            }
            Err(e) => {
                tracing::error!("[ERROR] Nightscout connection test failed: {}", e);
                let error_embed = CreateEmbed::new()
                    .title("Connection Failed")
                    .description("Could not connect to your Nightscout site. Please verify:\n• The URL is correct\n• Your site is publicly accessible\n• Your site is online")
                    .color(Colour::RED);

                let error_response = CreateInteractionResponseMessage::new()
                    .embed(error_embed)
                    .ephemeral(true);

                modal_response
                    .interaction
                    .create_response(context, CreateInteractionResponse::Message(error_response))
                    .await?;
            }
        }
    }

    Ok(())
}

pub async fn handle_button(
    handler: &Handler,
    context: &Context,
    interaction: &ComponentInteraction,
) -> anyhow::Result<()> {
    let is_private = match interaction.data.custom_id.as_str() {
        "setup_private" => true,
        "setup_public" => false,
        _ => return Ok(()),
    };

    let embed = interaction
        .message
        .embeds
        .first()
        .ok_or_else(|| anyhow::anyhow!("No embed found in message"))?;

    let url = embed
        .fields
        .iter()
        .find(|field| field.name == "nightscout_url")
        .map(|field| field.value.as_str())
        .ok_or_else(|| anyhow::anyhow!("Could not extract URL from message"))?;

    let token = embed
        .fields
        .iter()
        .find(|field| field.name == "nightscout_token")
        .map(|field| field.value.as_str())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_string());

    let nightscout_info = NightscoutInfo {
        nightscout_url: Some(url.to_string()),
        nightscout_token: token,
        allowed_people: vec![],
        is_private,
        microbolus_threshold: 0.5,
        display_microbolus: true,
    };

    let user_id = interaction.user.id.get();

    let db_result = match handler
        .database
        .insert_user(user_id, nightscout_info.clone())
        .await
    {
        Ok(v) => Ok(v),
        Err(_) => {
            handler
                .database
                .update_user(user_id, nightscout_info.clone())
                .await
        }
    };

    match db_result {
        Ok(_) => {
            let privacy_text = if is_private { "Private" } else { "Public" };
            let token_text = if nightscout_info.nightscout_token.is_some() {
                "\n[SECURE] **Access Token:** Configured securely"
            } else {
                "\n[OPEN] **Access Token:** None (requests without authentication)"
            };
            let success_embed = CreateEmbed::new()
                .title("Setup Complete")
                .description(format!(
                    "**URL:** {}\n**Privacy:** {}{}",
                    url, privacy_text, token_text
                ))
                .color(Colour::DARK_GREEN);

            let success_response = CreateInteractionResponseMessage::new()
                .embed(success_embed)
                .components(vec![])
                .ephemeral(true);
            interaction
                .create_response(
                    context,
                    CreateInteractionResponse::UpdateMessage(success_response),
                )
                .await?;
        }
        Err(e) => {
            let error_response = CreateInteractionResponseMessage::new()
                .embed(
                    CreateEmbed::new()
                        .title("Database Error")
                        .description(format!("Failed to save: {}", e))
                        .color(Colour::RED),
                )
                .ephemeral(true);
            interaction
                .create_response(
                    context,
                    CreateInteractionResponse::UpdateMessage(error_response),
                )
                .await?;
        }
    }

    Ok(())
}
async fn show_privacy_selection(
    context: &Context,
    modal_interaction: &serenity::all::ModalInteraction,
    url: &str,
    token: Option<String>,
) -> anyhow::Result<()> {
    let buttons = CreateActionRow::Buttons(vec![
        CreateButton::new("setup_public")
            .label("Public")
            .style(ButtonStyle::Success),
        CreateButton::new("setup_private")
            .label("Private")
            .style(ButtonStyle::Secondary),
    ]);

    let token_text = if token.is_some() {
        "\n\n[SECURE] **Access Token:** Will be saved securely (API-SECRET or Bearer)"
    } else {
        "\n\n[OPEN] **No Token:** Requests will be made without authentication"
    };

    let privacy_embed = CreateEmbed::new()
        .title("Privacy Settings")
        .description(format!("Choose who can see your blood glucose data for **{}**:\n\n**Public:** Anyone can use commands to see your data\n**Private:** Only you and people you specifically allow can see your data{}", url, token_text))
        .field("nightscout_url", url, false)
        .field("nightscout_token", token.as_deref().unwrap_or(""), false)
        .color(Colour::BLURPLE);

    let response = CreateInteractionResponseMessage::new()
        .embed(privacy_embed)
        .components(vec![buttons])
        .ephemeral(true);

    modal_interaction
        .create_response(context, CreateInteractionResponse::Message(response))
        .await?;

    Ok(())
}

fn validate_and_fix_url(input: &str) -> Result<String, String> {
    let mut url = input.trim().to_string();

    // Check for empty or whitespace-only input
    if url.is_empty() {
        return Err("URL cannot be empty".to_string());
    }

    // Check for obviously invalid patterns
    if url.contains(' ') {
        return Err("URL cannot contain spaces".to_string());
    }

    // Add https:// prefix if no scheme is present
    if !url.starts_with("http://") && !url.starts_with("https://") {
        url = format!("https://{}", url);
    }

    // Ensure URL ends with '/'
    if !url.ends_with('/') {
        url.push('/');
    }

    // Parse and validate the URL
    match Url::parse(&url) {
        Ok(parsed) => {
            // Check for required components
            if parsed.host().is_none() {
                return Err("URL must have a valid domain name".to_string());
            }

            // Ensure it's http or https
            match parsed.scheme() {
                "http" | "https" => {}
                _ => return Err("URL must use http or https protocol".to_string()),
            }

            // Additional validation: ensure domain has at least one dot (basic domain check)
            if let Some(host) = parsed.host_str()
                && !host.contains('.')
                && host != "localhost"
            {
                return Err("Invalid domain name format".to_string());
            }

            Ok(url)
        }
        Err(e) => match e {
            url::ParseError::RelativeUrlWithoutBase => {
                Err("Invalid URL: cannot be a relative path".to_string())
            }
            url::ParseError::InvalidDomainCharacter => {
                Err("Invalid characters in domain name".to_string())
            }
            url::ParseError::InvalidPort => Err("Invalid port number in URL".to_string()),
            _ => Err(format!("Invalid URL format: {}", e)),
        },
    }
}

pub fn register() -> CreateCommand {
    CreateCommand::new("setup")
        .description("Configure your Nightscout URL and privacy settings")
        .contexts(vec![
            InteractionContext::Guild,
            InteractionContext::PrivateChannel,
        ])
}
