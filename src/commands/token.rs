use crate::Handler;
use serenity::all::{
    Colour, CommandInteraction, Context, CreateCommand, CreateEmbed, CreateInputText,
    CreateInteractionResponse, CreateInteractionResponseMessage, CreateQuickModal,
    InteractionContext,
};

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
            .description("You need to run `/setup` first to configure your Nightscout URL before setting a token.");

        let msg = CreateInteractionResponseMessage::new().embed(error_embed);
        let builder = CreateInteractionResponse::Message(msg);
        interaction.create_response(&context.http, builder).await?;
        return Ok(());
    }

    let modal = CreateQuickModal::new("Nightscout API Token")
        .timeout(std::time::Duration::from_secs(600))
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
    let modal_response = response.unwrap();

    let token_input = &modal_response.inputs[0];
    let token = if token_input.trim().is_empty() {
        None
    } else {
        Some(token_input.trim().to_string())
    };

    let current_user_info = handler
        .database
        .get_user_info(interaction.user.id.get())
        .await?;

    let updated_nightscout_info = crate::utils::database::NightscoutInfo {
        nightscout_url: current_user_info.nightscout.nightscout_url,
        nightscout_token: token.clone(),
        is_private: current_user_info.nightscout.is_private,
        allowed_people: current_user_info.nightscout.allowed_people,
    };

    let user_id = interaction.user.id.get();
    match handler
        .database
        .update_user(user_id, updated_nightscout_info)
        .await
    {
        Ok(_) => {
            let (title, description, color) = if token.is_some() {
                (
                    "Token Updated",
                    "[OK] Your Nightscout access token has been updated successfully.\n\nThis token will be used to authenticate requests to your Nightscout site using either API-SECRET header or Bearer authorization depending on the token format.",
                    Colour::DARK_GREEN,
                )
            } else {
                (
                    "Token Removed",
                    "[OK] Your Nightscout access token has been removed.\n\nRequests will now be made without authentication.",
                    Colour::ORANGE,
                )
            };

            let success_embed = CreateEmbed::new()
                .title(title)
                .description(description)
                .color(color);

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
            eprintln!("Failed to update user token: {}", e);
            let error_embed = CreateEmbed::new()
                .title("Update Failed")
                .description("[ERROR] Failed to update your token. Please try again later.")
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

    Ok(())
}

pub fn register() -> CreateCommand {
    CreateCommand::new("token")
        .description("Set or update your Nightscout API token for authentication")
        .contexts(vec![
            InteractionContext::Guild,
            InteractionContext::PrivateChannel,
        ])
}
