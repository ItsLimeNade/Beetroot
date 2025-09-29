use crate::Handler;
use crate::utils::graph::draw_graph;
use serenity::all::{
    CommandInteraction, CommandOptionType, Context, CreateInteractionResponse,
    CreateInteractionResponseMessage, EditAttachments, EditInteractionResponse, InteractionContext,
    ResolvedOption, ResolvedValue, User,
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

    let mut hours = 3_i64;
    let mut target_user: Option<&User> = None;

    for option in &interaction.data.options() {
        match option {
            ResolvedOption {
                name: "hours",
                value: ResolvedValue::Integer(h),
                ..
            } => {
                hours = *h;
            }
            ResolvedOption {
                name: "user",
                value: ResolvedValue::User(user, _),
                ..
            } => {
                target_user = Some(user);
            }
            _ => {}
        }
    }

    let (user_data, _requesting_user_id, is_viewing_other_user) = if let Some(target) = target_user {
        let target_data = handler
            .database
            .get_user_info(target.id.get())
            .await
            .map_err(|_| anyhow::anyhow!("Target user not found in database"))?;

        if !target_data.nightscout.is_private {
            (target_data, interaction.user.id.get(), true)
        } else if target_data.nightscout.allowed_people.contains(&interaction.user.id.get()) {
            (target_data, interaction.user.id.get(), true)
        } else {
            crate::commands::error::edit_response(
                context,
                interaction,
                "You don't have permission to view this user's graph. The user has a private profile and hasn't authorized you.",
            )
            .await?;
            return Ok(());
        }
    } else {
        let data = handler
            .database
            .get_user_info(interaction.user.id.get())
            .await?;
        (data, interaction.user.id.get(), false)
    };

    let base_url = user_data
        .nightscout
        .nightscout_url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Nightscout URL missing"))?;

    // Validate URL before making requests
    if base_url.trim().is_empty() {
        let error_msg = if is_viewing_other_user {
            "The target user hasn't configured their Nightscout URL."
        } else {
            "Your Nightscout URL is empty. Please run `/setup` to configure it properly."
        };

        crate::commands::error::edit_response(context, interaction, error_msg).await?;
        return Ok(());
    }

    let token = user_data.nightscout.nightscout_token.as_deref();
    let entries = match handler
        .nightscout_client
        .get_entries_for_hours(base_url, hours as u16, token)
        .await
    {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("Failed to get entries for graph: {}", e);
            let error_msg = if is_viewing_other_user {
                "Could not fetch glucose data from the target user's Nightscout site."
            } else {
                "Could not fetch glucose data from your Nightscout site. Please check your URL configuration with `/setup`."
            };

            crate::commands::error::edit_response(context, interaction, error_msg).await?;
            return Ok(());
        }
    };

    let profile = match handler.nightscout_client.get_profile(base_url, token).await {
        Ok(profile) => profile,
        Err(e) => {
            eprintln!("Failed to get profile for graph: {}", e);
            crate::utils::nightscout::Profile {
                default_profile: "default".to_string(),
                store: std::collections::HashMap::new(),
            }
        }
    };

    let now = chrono::Utc::now();
    let hours_ago = now - chrono::Duration::hours(hours);
    let start_time = hours_ago.to_rfc3339();
    let end_time = now.to_rfc3339();

    let treatments = match handler
        .nightscout_client
        .fetch_treatments_between(base_url, &start_time, &end_time, token)
        .await
    {
        Ok(treatments) => treatments,
        Err(e) => {
            eprintln!("Failed to get treatments for graph: {}", e);
            vec![]
        }
    };

    let buffer = draw_graph(&entries, &treatments, &profile, &user_data.nightscout, handler, hours as u16, None)?;

    let graph_attachment = CreateAttachment::bytes(buffer, "graph.png");
    let graph_edit_attachment = EditAttachments::new().add(graph_attachment);

    // Send only the graph with no message
    let message = EditInteractionResponse::new().attachments(graph_edit_attachment);

    interaction.edit_response(&context.http, message).await?;

    Ok(())
}

pub fn register() -> CreateCommand {
    CreateCommand::new("graph")
        .description("Sends a graph of blood glucose data.")
        .add_option(
            CreateCommandOption::new(CommandOptionType::Integer, "hours", "3h to 24h of data.")
                .min_int_value(3)
                .max_int_value(24)
                .required(false),
        )
        .add_option(
            CreateCommandOption::new(CommandOptionType::User, "user", "View another user's graph (requires permission).")
                .required(false),
        )
        .contexts(vec![
            InteractionContext::Guild,
            InteractionContext::PrivateChannel,
        ])
}
