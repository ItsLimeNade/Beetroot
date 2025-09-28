use serenity::all::{
    CommandInteraction, Context, CreateInteractionResponse, CreateInteractionResponseMessage,
};

pub async fn run(
    context: &Context,
    interaction: &CommandInteraction,
    error_message: &str,
) -> anyhow::Result<()> {
    let message =
        CreateInteractionResponseMessage::new().content(format!("[ERROR] {}", error_message));

    let builder = CreateInteractionResponse::Message(message);
    interaction
        .create_response(&context.http, builder)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to send error response: {}", e))?;

    Ok(())
}
