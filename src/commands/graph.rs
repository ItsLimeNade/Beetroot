use crate::Handler;
use crate::utils::graph::draw_graph;
use serenity::all::{
    CommandInteraction, CommandOptionType, Context, CreateInteractionResponse,
    CreateInteractionResponseMessage, EditAttachments, EditInteractionResponse, InteractionContext,
    ResolvedOption, ResolvedValue,
};
use serenity::builder::{CreateAttachment, CreateCommand, CreateCommandOption};

pub async fn run(
    handler: &Handler,
    context: &Context,
    interaction: &CommandInteraction,
) -> anyhow::Result<()> {
    // IMMEDIATELY defer the response (within 3 seconds)
    interaction
        .create_response(
            &context.http,
            CreateInteractionResponse::Defer(CreateInteractionResponseMessage::new()),
        )
        .await?;

    let user_data = handler
        .database
        .get_user_info(interaction.user.id.get())
        .await?;

    let base_url = user_data
        .nightscout
        .nightscout_url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Nightscout URL missing"))?;

    // Validate URL before making requests
    if base_url.trim().is_empty() {
        let error_message = EditInteractionResponse::new().content(
            "[ERROR] Your Nightscout URL is empty. Please run `/setup` to configure it properly.",
        );
        interaction
            .edit_response(&context.http, error_message)
            .await?;
        return Ok(());
    }

    let hours = if let Some(ResolvedOption {
        value: ResolvedValue::Integer(hours),
        ..
    }) = interaction.data.options().first()
    {
        hours.clone()
    } else {
        3_i64
    };

    let token = user_data.nightscout.nightscout_token.as_deref();
    let entries = match handler
        .nightscout_client
        .get_entries_for_hours(base_url, hours as u8, token)
        .await
    {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("Failed to get entries for graph: {}", e);
            let error_message = EditInteractionResponse::new()
                .content("[ERROR] Could not fetch glucose data from your Nightscout site. Please check your URL configuration with `/setup`.");
            interaction
                .edit_response(&context.http, error_message)
                .await?;
            return Ok(());
        }
    };

    let profile = match handler.nightscout_client.get_profile(base_url, token).await {
        Ok(profile) => profile,
        Err(e) => {
            eprintln!("Failed to get profile for graph: {}", e);
            // Use default profile if we can't fetch it
            crate::utils::nightscout::Profile {
                default_profile: "default".to_string(),
                store: std::collections::HashMap::new(),
            }
        }
    };

    let buffer = draw_graph(&entries, &profile, &handler, None)?;

    let graph_attachment = CreateAttachment::bytes(buffer, "graph.png");
    let graph_edit_attachment = EditAttachments::new().add(graph_attachment);

    // Use followup instead of create_response
    let message = EditInteractionResponse::new().attachments(graph_edit_attachment);

    interaction.edit_response(&context.http, message).await?;

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
