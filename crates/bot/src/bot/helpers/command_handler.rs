use crate::bot::Handler;
use crate::commands;
use anyhow::Result;
use serenity::all::{CommandInteraction, Context};

/// List of commands that don't require user setup
const UNRESTRICTED_COMMANDS: &[&str] = &["setup", "convert", "help"];

/// Route a slash command to its handler
pub async fn handle_slash_command(
    handler: &Handler,
    context: &Context,
    command: &CommandInteraction,
) -> Result<()> {
    // Check if user exists for restricted commands
    let user_exists = handler.database.user_exists(command.user.id.get()).await?;

    if !user_exists && !UNRESTRICTED_COMMANDS.contains(&command.data.name.as_str()) {
        return commands::error::run(
            context,
            command,
            "You need to register your Nightscout URL first. Use `/setup` to get started.",
        )
        .await;
    }

    // Route to appropriate command handler
    match command.data.name.as_str() {
        "allow" => commands::allow::run(handler, context, command).await,
        "bg" => commands::bg::run(handler, context, command).await,
        "convert" => commands::convert::run(handler, context, command).await,
        "get-nightscout-url" => commands::get_nightscout_url::run(handler, context, command).await,
        "graph" => commands::graph::run(handler, context, command).await,
        "help" => commands::help::run(handler, context, command).await,
        "info" => commands::info::run(handler, context, command).await,
        "set-nightscout-url" => commands::set_nightscout_url::run(handler, context, command).await,
        "set-threshold" => commands::set_threshold::run(handler, context, command).await,
        "set-token" => commands::set_token::run(handler, context, command).await,
        "set-visibility" => commands::set_visibility::run(handler, context, command).await,
        "setup" => commands::setup::run(handler, context, command).await,
        "stickers" => commands::stickers::run(handler, context, command).await,
        "token" => commands::token::run(handler, context, command).await,
        unknown_command => {
            eprintln!("Unknown slash command received: '{}'", unknown_command);
            commands::error::run(
                context,
                command,
                &format!(
                    "Unknown command: `{}`. Use `/help` to see all available commands.",
                    unknown_command
                ),
            )
            .await
        }
    }
}

/// Route a context menu command to its handler
pub async fn handle_context_command(
    handler: &Handler,
    context: &Context,
    command: &CommandInteraction,
) -> Result<()> {
    match command.data.name.as_str() {
        "Add Sticker" => commands::add_sticker::run(handler, context, command).await,
        "Analyze Units" => commands::analyze_units::run(handler, context, command).await,
        unknown_context_command => {
            eprintln!(
                "Unknown context menu command received: '{}'",
                unknown_context_command
            );
            commands::error::run(
                context,
                command,
                &format!(
                    "Unknown context menu command: `{}`",
                    unknown_context_command
                ),
            )
            .await
        }
    }
}
