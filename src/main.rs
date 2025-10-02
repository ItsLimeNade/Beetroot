mod commands;
mod utils;

use ab_glyph::FontArc;
use anyhow::anyhow;
use serenity::all::{
    Command, CreateInteractionResponse, CreateInteractionResponseMessage, Interaction, Ready,
};
use serenity::prelude::*;

use crate::utils::database::Database;
use crate::utils::nightscout::Nightscout;

#[allow(dead_code)]
pub struct Handler {
    nightscout_client: Nightscout,
    database: Database,
    font: FontArc,
}

impl Handler {
    async fn new() -> Self {
        let font_bytes = std::fs::read("assets/fonts/GeistMono-Regular.ttf")
            .map_err(|e| anyhow!("Failed to read font: {}", e))
            .unwrap();
        Handler {
            nightscout_client: Nightscout::new(),
            database: Database::new().await.unwrap(),
            font: FontArc::try_from_vec(font_bytes)
                .map_err(|_| anyhow!("Failed to parse font"))
                .unwrap(),
        }
    }
}

#[serenity::async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, context: Context, interaction: Interaction) {
        let result = match interaction {
            Interaction::Command(ref command) => {
                // Handle context menu commands
                if command.data.kind == serenity::model::application::CommandType::Message {
                    match command.data.name.as_str() {
                        "Add Sticker" => commands::add_sticker::run(self, &context, command).await,
                        "Analyze Units" => {
                            commands::analyze_units::run(self, &context, command).await
                        }
                        unknown_context_command => {
                            eprintln!(
                                "Unknown context menu command received: '{}'",
                                unknown_context_command
                            );
                            commands::error::run(
                                &context,
                                command,
                                &format!(
                                    "Unknown context menu command: `{}`",
                                    unknown_context_command
                                ),
                            )
                            .await
                        }
                    }
                } else {
                    // Handle regular slash commands
                    match self.database.user_exists(command.user.id.get()).await {
                        Ok(exists) => {
                            if !exists
                                && !["setup", "convert", "help"]
                                    .contains(&command.data.name.as_str())
                            {
                                commands::error::run(&context, command, "You need to register your Nightscout URL first. Use `/setup` to get started.").await
                            } else {
                                match command.data.name.as_str() {
                                    "allow" => commands::allow::run(self, &context, command).await,
                                    "bg" => commands::bg::run(self, &context, command).await,
                                    "convert" => {
                                        commands::convert::run(self, &context, command).await
                                    }
                                    "graph" => commands::graph::run(self, &context, command).await,
                                    "help" => commands::help::run(self, &context, command).await,
                                    "info" => commands::info::run(self, &context, command).await,
                                    "setup" => commands::setup::run(self, &context, command).await,
                                    "stickers" => {
                                        commands::sticker::run(self, &context, command).await
                                    }
                                    "set-threshold" => {
                                        commands::set_threshold::run(self, &context, command).await
                                    }
                                    "token" => commands::token::run(self, &context, command).await,
                                    unknown_command => {
                                        eprintln!(
                                            "Unknown slash command received: '{}'",
                                            unknown_command
                                        );
                                        commands::error::run(
                                            &context,
                                            command,
                                            &format!("Unknown command: `{}`. Available commands are: `/allow`, `/bg`, `/convert`, `/graph`, `/help`, `/info`, `/setup`, `/set-threshold`, `/stickers`, `/token`", unknown_command)
                                        ).await
                                    }
                                }
                            }
                        }
                        Err(db_error) => Err(anyhow::anyhow!("Database error: {}", db_error)),
                    }
                }
            }
            Interaction::Component(ref component) => match component.data.custom_id.as_str() {
                "setup_private" | "setup_public" => {
                    commands::setup::handle_button(self, &context, component).await
                }
                id if id.starts_with("help_page_") => {
                    commands::help::handle_button(self, &context, component).await
                }
                id if id.starts_with("remove_sticker_") || id == "clear_all_stickers" => {
                    commands::sticker::handle_button(self, &context, component).await
                }
                _ => Ok(()),
            },
            _ => Ok(()),
        };

        if let Err(e) = result {
            let error_msg = format!("There was an error processing your interaction: {}", e);
            eprintln!("ERROR: {}", error_msg);

            match &interaction {
                Interaction::Command(command) => {
                    if let Err(send_err) = commands::error::run(
                        &context,
                        command,
                        "An unexpected error occurred. Please try again later.",
                    )
                    .await
                    {
                        eprintln!("Failed to send error response to user: {}", send_err);
                    }
                }
                Interaction::Component(component) => {
                    let error_response = CreateInteractionResponseMessage::new()
                        .content("[ERROR] An unexpected error occurred. Please try again later.")
                        .ephemeral(true);
                    if let Err(send_err) = component
                        .create_response(
                            &context.http,
                            CreateInteractionResponse::Message(error_response),
                        )
                        .await
                    {
                        eprintln!(
                            "Failed to send component error response to user: {}",
                            send_err
                        );
                    }
                }
                _ => {
                    eprintln!("Unhandled interaction type in error handler");
                }
            }
        }
    }

    async fn ready(&self, context: Context, ready: Ready) {
        tracing::info!("[BOT] {} is ready and connected!", ready.user.name);
        let commands_vec = vec![
            // Slash commands
            commands::allow::register(),
            commands::bg::register(),
            commands::convert::register(),
            commands::graph::register(),
            commands::help::register(),
            commands::info::register(),
            commands::setup::register(),
            commands::set_threshold::register(),
            commands::sticker::register(),
            commands::token::register(),
            // Context menu commands
            commands::add_sticker::register(),
            commands::analyze_units::register(),
        ];
        let command_count = commands_vec.len();
        let commands = Command::set_global_commands(&context, commands_vec).await;
        tracing::info!(
            "[CMD] Successfully registered {} global slash commands",
            command_count
        );
        tracing::debug!("Registered commands: {:#?}", commands);
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("beetroot=debug,info")),
        )
        .with_target(false)
        .with_thread_ids(false)
        .with_file(true)
        .with_line_number(true)
        .init();

    tracing::info!("[INIT] Starting Beetroot Discord Bot");

    let token = dotenvy::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    let handler = Handler::new().await;
    let mut client = Client::builder(token, GatewayIntents::empty())
        .event_handler(handler)
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await {
        tracing::error!("[ERROR] Discord client error: {why:?}");
    }

    Ok(())
}
