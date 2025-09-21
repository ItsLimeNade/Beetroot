mod commands;
mod utils;
mod tests;

use serenity::prelude::*;
use serenity::all::{Command, CreateInteractionResponse, CreateInteractionResponseMessage, Interaction, Ready};

struct Handler;

#[serenity::async_trait]
impl EventHandler for Handler {

    async fn interaction_create(&self, context: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            println!("Received command interaction: {command:#?}");

            let content = match command.data.name.as_str() {
                _ => Some("Not implemented :(".to_string())
            };

            if let Some(content) = content {
                let data = CreateInteractionResponseMessage::new().content(content);
                let builder = CreateInteractionResponse::Message(data);
                if let Err(why) = command.create_response(&context.http, builder).await {
                    println!("Cannot respond to slash command: {why}");
                }
            }
        }
    }

    async fn ready(&self, context: Context, ready: Ready) {
        println!("{} is ready!", ready.user.name);

        let commands = Command::set_global_commands(&context, vec![
            // Commands here
        ]).await;

        println!("Successfully registered following global slash command: {commands:#?}");
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let token = dotenvy::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let mut client = Client::builder(token, GatewayIntents::empty())
    .event_handler(Handler)
    .await
    .expect("Error creating client");

    if let Err(why) = client.start().await {
        println!("Client error: {why:?}");
    }

    Ok(())
}
