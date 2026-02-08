use crate::bot::{
    Handler, command_registry, component_router, helpers::command_handler, version_checker,
};
use crate::commands;
use serenity::all::{
    Command, CreateInteractionResponse, CreateInteractionResponseMessage, Interaction, Ready,
};
use serenity::prelude::*;

#[serenity::async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, context: Context, interaction: Interaction) {
        let result = match interaction {
            Interaction::Command(ref command) => {
                // Determine if it's a context menu or slash command
                let command_result =
                    if command.data.kind == serenity::model::application::CommandType::Message {
                        command_handler::handle_context_command(self, &context, command).await
                    } else {
                        command_handler::handle_slash_command(self, &context, command).await
                    };

                // Check for version updates after successful command execution
                if command_result.is_ok()
                    && let Ok(exists) = self.database.user_exists(command.user.id.get()).await
                    && exists
                {
                    let _ =
                        version_checker::check_and_notify_version_update(self, &context, command)
                            .await;
                }

                command_result
            }

            Interaction::Component(ref component) => {
                component_router::route_component_interaction(self, &context, component).await
            }

            _ => Ok(()),
        };

        // Handle errors
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

        let commands_vec = command_registry::get_all_commands();
        let command_count = commands_vec.len();

        let _commands = Command::set_global_commands(&context, commands_vec).await;

        tracing::info!(
            "[CMD] Successfully registered {} global commands",
            command_count
        );
    }
}
