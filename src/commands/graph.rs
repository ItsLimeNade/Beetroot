use crate::Handler;
use crate::utils::graph::draw_graph;
use serenity::all::{
    CommandInteraction, CommandOptionType, Context, CreateInteractionResponse,
    CreateInteractionResponseMessage, InteractionContext, ResolvedOption, ResolvedValue,
};
use serenity::builder::{CreateAttachment, CreateCommand, CreateCommandOption};

pub async fn run(
    handler: &Handler,
    context: &Context,
    interaction: &CommandInteraction,
) -> anyhow::Result<()> {
    let hours = if let Some(ResolvedOption {
        value: ResolvedValue::Integer(hours),
        ..
    }) = interaction.data.options().first()
    {
        hours.clone()
    } else {
        3_i64
    };

    let entries = handler
        .nightscout_client
        .get_entries_for_hours(base_url, hours as u8)
        .await?;

    let profile = handler.nightscout_client.get_profile(base_url).await?;

    let buffer = draw_graph(&entries, &profile, None)?;

    let graph_attachement = CreateAttachment::bytes(buffer, "graph.png");

    let message = CreateInteractionResponseMessage::new()
        .add_file(graph_attachement);

    let builder = CreateInteractionResponse::Message(message);
    if let Err(why) = interaction.create_response(&context.http, builder).await {
        println!("Cannot respond to slash command: {why}");
    }

    Ok(())
}

pub fn register() -> CreateCommand {
    CreateCommand::new("graph")
        .description("Sends a graph of your latest blood glucose.")
        .add_option(
            CreateCommandOption::new(CommandOptionType::Integer, "hours", "3h to 24h of data.")
                .min_int_value(3)
                .max_int_value(24)
                .required(false),
        )
        .contexts(vec![
            InteractionContext::Guild,
            InteractionContext::PrivateChannel,
        ])
}
