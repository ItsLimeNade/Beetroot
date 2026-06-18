use crate::data::{Context, Error};
use crate::utils::emojis;
use crate::utils::net::parse_and_normalize_url;
use macros::track_analytics;
use poise::Modal;
use poise::serenity_prelude as serenity;
use serenity::{
    ButtonStyle, Colour, CreateActionRow, CreateButton, CreateEmbed, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};

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
#[track_analytics("setup")]
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
            tracing::debug!(
                user = %crate::logging::redact(ctx.author().id.get()),
                reason = %e,
                "rejected nightscout URL during setup"
            );
            send_error!(ctx, "Invalid URL", e);
            return Ok(());
        }
    };

    let url_str = url_obj.to_string();

    ctx.defer_ephemeral().await?;

    tracing::debug!(
        user = %crate::logging::redact(ctx.author().id.get()),
        url = %crate::logging::redact(&url_str),
        has_token = modal_data.nightscout_token.is_some(),
        "verifying nightscout connection"
    );
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
        format!(
            "\n\n{} **Access Token:** Securely Encrypted",
            emojis::password()
        )
    } else {
        format!("\n\n{} **No Token:** Public Access", emojis::lock_open())
    };

    let embed = CreateEmbed::new()
        .title(format!("{} Privacy Settings", emojis::lock_closed()))
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
                tracing::info!(
                    user = %crate::logging::redact(ctx.author().id.get()),
                    is_private,
                    has_token = token.is_some(),
                    "nightscout setup completed"
                );
                let privacy_text = if is_private { "Private" } else { "Public" };
                let success_embed = CreateEmbed::new()
                    .title(format!("{} Setup Complete", emojis::celebration()))
                    .description(format!(
                        "{} Nightscout configured successfully!\n\n**URL:** {}\n**Privacy:** {}",
                        emojis::wifi(),
                        url,
                        privacy_text
                    ))
                    .field(
                        format!("{} Not medical advice", emojis::warning()),
                        "**Beetroot is not a medical device and does not give medical advice.** \
                         Readings can be delayed or wrong. Always confirm with a blood glucose \
                         meter and follow your healthcare provider before making any treatment \
                         decision.",
                        false,
                    )
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
                tracing::error!(
                    user = %crate::logging::redact(ctx.author().id.get()),
                    error = %e,
                    "failed to save nightscout setup"
                );
                mci.create_response(
                    ctx.serenity_context(),
                    CreateInteractionResponse::Message(
                        serenity::CreateInteractionResponseMessage::new()
                            .content(format!(
                                "{} Database error. Please try again.",
                                emojis::error()
                            ))
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
                            .title(format!("{} Timed Out", emojis::sync_problem()))
                            .description("Setup timed out. Please run `/setup` again.")
                            .color(Colour::RED),
                    )
                    .components(vec![]),
            )
            .await?;
    }

    Ok(())
}
