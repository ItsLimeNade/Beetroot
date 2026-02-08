use crate::bot::Handler;
use serenity::all::{
    Colour, CommandInteraction, Context, CreateEmbed, CreateInteractionResponse,
    CreateInteractionResponseMessage, InteractionContext,
};
use serenity::builder::CreateCommand;

pub async fn run(
    _handler: &Handler,
    context: &Context,
    interaction: &CommandInteraction,
) -> anyhow::Result<()> {
    let embed = CreateEmbed::new()
        .title("Beetroot - Blood Glucose Discord Bot")
        .description("A Discord bot for sharing and viewing blood glucose data from Nightscout.")
        .color(Colour::from_rgb(34, 197, 94))
        .field(
            "GitHub Repository",
            "[ItsLimeNade/Beetroot](https://github.com/ItsLimeNade/Beetroot)",
            true,
        )
        .field(
            "Report Issues",
            "Found a bug or have a feature request? Please create an issue on GitHub!",
            false,
        )
        .field(
            "Getting Started",
            "Use `/setup` to configure your Nightscout URL and get started!",
            false,
        )
        .thumbnail("https://raw.githubusercontent.com/ItsLimeNade/Beetroot/main/assets/images/beetroot.png")
        .footer(serenity::all::CreateEmbedFooter::new(
            "Open source and community driven"
        ));

    let response = CreateInteractionResponseMessage::new()
        .embed(embed)
        .ephemeral(true);

    interaction
        .create_response(context, CreateInteractionResponse::Message(response))
        .await?;

    Ok(())
}

pub fn register() -> CreateCommand {
    CreateCommand::new("info")
        .description("Show information about Beetroot bot and GitHub repository")
        .contexts(vec![
            InteractionContext::Guild,
            InteractionContext::PrivateChannel,
        ])
}
