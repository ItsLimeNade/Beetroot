use crate::Handler;
use serenity::all::{
    Colour, CommandInteraction, CommandOptionType, Context, CreateEmbed, CreateInteractionResponse,
    CreateInteractionResponseMessage, InteractionContext, ResolvedOption, ResolvedValue, User,
};
use serenity::builder::{CreateCommand, CreateCommandOption};

pub async fn run(
    handler: &Handler,
    context: &Context,
    interaction: &CommandInteraction,
) -> anyhow::Result<()> {
    let mut target_user: Option<&User> = None;
    let mut action = "add";

    for option in &interaction.data.options() {
        match option {
            ResolvedOption {
                name: "user",
                value: ResolvedValue::User(user, _),
                ..
            } => {
                target_user = Some(user);
            }
            ResolvedOption {
                name: "action",
                value: ResolvedValue::String(act),
                ..
            } => {
                action = act;
            }
            _ => {}
        }
    }

    let target_user = target_user.ok_or_else(|| anyhow::anyhow!("User parameter is required"))?;

    if target_user.id.get() == interaction.user.id.get() {
        crate::commands::error::run(
            context,
            interaction,
            "You cannot add or remove yourself from your own allowed users list.",
        )
        .await?;
        return Ok(());
    }

    if !handler
        .database
        .user_exists(interaction.user.id.get())
        .await?
    {
        crate::commands::error::run(
            context,
            interaction,
            "You need to run `/setup` first to configure your Nightscout before managing allowed users.",
        )
        .await?;
        return Ok(());
    }

    let result = match action {
        "add" => {
            handler
                .database
                .add_allowed_user(interaction.user.id.get(), target_user.id.get())
                .await
        }
        "remove" => {
            handler
                .database
                .remove_allowed_user(interaction.user.id.get(), target_user.id.get())
                .await
        }
        _ => {
            crate::commands::error::run(
                context,
                interaction,
                "Invalid action. Use 'add' or 'remove'.",
            )
            .await?;
            return Ok(());
        }
    };

    match result {
        Ok(true) => {
            let (title, description, color) = match action {
                "add" => (
                    "User Added",
                    format!(
                        "{} has been added to your allowed users list. They can now view your blood glucose data.",
                        target_user.display_name()
                    ),
                    Colour::from_rgb(34, 197, 94),
                ),
                "remove" => (
                    "User Removed",
                    format!(
                        "{} has been removed from your allowed users list. They can no longer view your blood glucose data.",
                        target_user.display_name()
                    ),
                    Colour::from_rgb(249, 115, 22),
                ),
                _ => unreachable!(),
            };

            let embed = CreateEmbed::new()
                .title(title)
                .description(description)
                .color(color);

            let response = CreateInteractionResponseMessage::new()
                .embed(embed)
                .ephemeral(true);

            interaction
                .create_response(context, CreateInteractionResponse::Message(response))
                .await?;
        }
        Ok(false) => {
            let message = match action {
                "add" => format!("{} is already in your allowed users list.", target_user.display_name()),
                "remove" => format!("{} is not in your allowed users list.", target_user.display_name()),
                _ => unreachable!(),
            };

            crate::commands::error::run(context, interaction, &message).await?;
        }
        Err(e) => {
            eprintln!("Database error in allow command: {}", e);
            crate::commands::error::run(
                context,
                interaction,
                "Failed to update allowed users list. Please try again later.",
            )
            .await?;
        }
    }

    Ok(())
}

pub fn register() -> CreateCommand {
    CreateCommand::new("allow")
        .description("Manage who can view your blood glucose data")
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::User,
                "user",
                "User to add or remove from your allowed list"
            )
            .required(true),
        )
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::String,
                "action",
                "Whether to add or remove the user"
            )
            .add_string_choice("Add user", "add")
            .add_string_choice("Remove user", "remove")
            .required(false),
        )
        .contexts(vec![
            InteractionContext::Guild,
            InteractionContext::PrivateChannel,
        ])
}