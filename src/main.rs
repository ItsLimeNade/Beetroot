mod commands;
mod tests;
mod utils;

use serenity::all::{
    Command, CreateInteractionResponse, CreateInteractionResponseMessage, Interaction, Ready,
};
use serenity::prelude::*;

use crate::utils::nightscout::Nightscout;

#[allow(dead_code)]
pub struct Handler {
    nightscout_client: Nightscout,
}

impl Handler {
    fn new() -> Self {
        Handler {
            nightscout_client: Nightscout::new(),
        }
    }
}

#[serenity::async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, context: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            println!("Received command interaction: {command:#?}");

            let result = match command.data.name.as_str() {
                "bg" => commands::bg::run(self, &context, &command).await,
                "graph" => commands::graph::run(self, &context, &command).await,
                _ => {
                    let data =
                        CreateInteractionResponseMessage::new().content("Not implemented :(");
                    let builder = CreateInteractionResponse::Message(data);
                    command
                        .create_response(&context.http, builder)
                        .await
                        .map_err(|e| anyhow::anyhow!("Failed to send response: {}", e))
                }
            };

            if let Err(e) = result {
                let error_msg = format!("There was an error processing your command: {}", e);
                let data = CreateInteractionResponseMessage::new().content(error_msg);
                let builder = CreateInteractionResponse::Message(data);

                if let Err(why) = command.create_response(&context.http, builder).await {
                    println!("Cannot respond to slash command with error: {why}");
                }
            }
        }
    }

    async fn ready(&self, context: Context, ready: Ready) {
        println!("{} is ready!", ready.user.name);
        let commands_vec = vec![commands::graph::register(), commands::bg::register()];
        let commands = Command::set_global_commands(&context, commands_vec).await;
        println!("Successfully registered following global slash command: {commands:#?}");
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let token = dotenvy::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let mut client = Client::builder(token, GatewayIntents::empty())
        .event_handler(Handler::new())
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await {
        println!("Client error: {why:?}");
    }

    Ok(())
}
