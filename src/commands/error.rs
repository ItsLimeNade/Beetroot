use serenity::all::{
    Colour, CommandInteraction, Context, CreateEmbed, CreateInteractionResponse, CreateInteractionResponseMessage,
    EditInteractionResponse,
};

pub async fn run(
    context: &Context,
    interaction: &CommandInteraction,
    error_message: &str,
) -> anyhow::Result<()> {
    let embed = CreateEmbed::new()
        .title("Error")
        .description(error_message)
        .color(Colour::RED);

    let message = CreateInteractionResponseMessage::new()
        .embed(embed)
        .ephemeral(true);

    let builder = CreateInteractionResponse::Message(message);
    interaction
        .create_response(&context.http, builder)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to send error response: {}", e))?;

    Ok(())
}

pub async fn edit_response(
    context: &Context,
    interaction: &CommandInteraction,
    error_message: &str,
) -> anyhow::Result<()> {
    let embed = CreateEmbed::new()
        .title("Error")
        .description(error_message)
        .color(Colour::RED);

    let edit_message = EditInteractionResponse::new().embed(embed);
    interaction
        .edit_response(&context.http, edit_message)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to edit error response: {}", e))?;

    Ok(())
}
