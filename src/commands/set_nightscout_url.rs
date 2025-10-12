use crate::Handler;
use serenity::all::{
    Colour, CommandInteraction, Context, CreateCommand, CreateEmbed, CreateInteractionResponse,
    CreateInteractionResponseMessage, CreateQuickModal, InteractionContext,
};
use url::Url;

pub async fn run(
    handler: &Handler,
    context: &Context,
    interaction: &CommandInteraction,
) -> anyhow::Result<()> {
    if !handler
        .database
        .user_exists(interaction.user.id.get())
        .await?
    {
        let error_embed = CreateEmbed::new()
            .color(Colour::RED)
            .title("Not Set Up")
            .description("You need to run `/setup` first to configure your Nightscout settings.");

        let msg = CreateInteractionResponseMessage::new()
            .embed(error_embed)
            .ephemeral(true);
        let builder = CreateInteractionResponse::Message(msg);
        interaction.create_response(&context.http, builder).await?;
        return Ok(());
    }

    let modal = CreateQuickModal::new("Update Nightscout URL")
        .timeout(std::time::Duration::from_secs(300))
        .short_field("New Nightscout URL");

    let response = interaction.quick_modal(context, modal).await?;

    if let Some(modal_response) = response {
        let url_input = &modal_response.inputs[0];

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

        // Test the connection
        let current_user_info = handler
            .database
            .get_user_info(interaction.user.id.get())
            .await?;

        tracing::info!(
            "[TEST] Testing Nightscout connection for URL: {}",
            validated_url
        );
        match handler
            .nightscout_client
            .get_entry(
                &validated_url,
                current_user_info.nightscout.nightscout_token.as_deref(),
            )
            .await
        {
            Ok(_) => {
                tracing::info!("[OK] Nightscout connection test successful");

                // Update the URL
                let updated_nightscout_info = crate::utils::database::NightscoutInfo {
                    nightscout_url: Some(validated_url.clone()),
                    nightscout_token: current_user_info.nightscout.nightscout_token,
                    is_private: current_user_info.nightscout.is_private,
                    allowed_people: current_user_info.nightscout.allowed_people,
                    microbolus_threshold: current_user_info.nightscout.microbolus_threshold,
                    display_microbolus: current_user_info.nightscout.display_microbolus,
                };

                let user_id = interaction.user.id.get();
                match handler
                    .database
                    .update_user(user_id, updated_nightscout_info)
                    .await
                {
                    Ok(_) => {
                        let success_embed = CreateEmbed::new()
                            .title("URL Updated")
                            .description(format!(
                                "[OK] Your Nightscout URL has been updated successfully.\n\n**New URL:** {}",
                                validated_url
                            ))
                            .color(Colour::DARK_GREEN);

                        let success_response = CreateInteractionResponseMessage::new()
                            .embed(success_embed)
                            .ephemeral(true);

                        modal_response
                            .interaction
                            .create_response(
                                context,
                                CreateInteractionResponse::Message(success_response),
                            )
                            .await?;
                    }
                    Err(e) => {
                        eprintln!("Failed to update user URL: {}", e);
                        let error_embed = CreateEmbed::new()
                            .title("Update Failed")
                            .description(
                                "[ERROR] Failed to update your URL. Please try again later.",
                            )
                            .color(Colour::RED);

                        let error_response = CreateInteractionResponseMessage::new()
                            .embed(error_embed)
                            .ephemeral(true);

                        modal_response
                            .interaction
                            .create_response(
                                context,
                                CreateInteractionResponse::Message(error_response),
                            )
                            .await?;
                    }
                }
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
    CreateCommand::new("set-nightscout-url")
        .description("Update your Nightscout URL")
        .contexts(vec![
            InteractionContext::Guild,
            InteractionContext::PrivateChannel,
        ])
}
