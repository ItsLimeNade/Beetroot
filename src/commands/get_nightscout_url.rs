use crate::Handler;
use serenity::all::{
    Colour, CommandInteraction, Context, CreateCommand, CreateEmbed, CreateInteractionResponse,
    CreateInteractionResponseMessage, InteractionContext,
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
            .description("You need to run `/setup` first to configure your Nightscout URL.");

        let msg = CreateInteractionResponseMessage::new()
            .embed(error_embed)
            .ephemeral(true);
        let builder = CreateInteractionResponse::Message(msg);
        interaction.create_response(&context.http, builder).await?;
        return Ok(());
    }

    let user_info = handler
        .database
        .get_user_info(interaction.user.id.get())
        .await?;

    let url = user_info
        .nightscout
        .nightscout_url
        .unwrap_or_else(|| "Not set".to_string());

    let has_token = user_info.nightscout.nightscout_token.is_some();
    let token_status = if has_token {
        "[SECURE] Token is configured"
    } else {
        "[OPEN] No token configured"
    };

    let embed = CreateEmbed::new()
        .title("Your Nightscout Configuration")
        .description(format!(
            "**URL:** {}\n**Token Status:** {}",
            url, token_status
        ))
        .color(Colour::BLURPLE);

    let msg = CreateInteractionResponseMessage::new()
        .embed(embed)
        .ephemeral(true);
    let builder = CreateInteractionResponse::Message(msg);
    interaction.create_response(&context.http, builder).await?;

    Ok(())
}

pub fn register() -> CreateCommand {
    CreateCommand::new("get-nightscout-url")
        .description("View your current Nightscout URL and token status")
        .contexts(vec![
            InteractionContext::Guild,
            InteractionContext::PrivateChannel,
        ])
}
