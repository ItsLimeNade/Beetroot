mod commands;
mod tests;
mod utils;

use chrono::{Duration, Local};
use rand::Rng;
use serenity::all::{
    Command, CreateInteractionResponse, CreateInteractionResponseMessage, Interaction, Ready,
};
use serenity::prelude::*;

use crate::utils::nightscout::Entry;

#[allow(dead_code)]
struct Handler;

#[serenity::async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, context: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            println!("Received command interaction: {command:#?}");

            #[allow(clippy::match_single_binding)]
            let content = match command.data.name.as_str() {
                // Commands here.
                _ => Some("Not implemented :(".to_string()),
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

        let commands = Command::set_global_commands(
            &context,
            vec![
                // Commands here
            ],
        )
        .await;

        println!("Successfully registered following global slash command: {commands:#?}");
    }
}

#[allow(dead_code)]
fn mock_entries() -> Vec<Entry> {
    let now = Local::now();
    let mut rng = rand::rng();
    let mut entries = Vec::new();

    let mut current_glucose = rng.random_range(70.0..200.0);

    for i in 0..2 {
        let minutes_ago = i * 5;
        let t = now - Duration::minutes(minutes_ago as i64);

        if i > 0 {
            let change = rng.random_range(-10.0..10.0);
            current_glucose += change;
            current_glucose = (current_glucose as f32).clamp(50.0, 300.0);
        }

        entries.push(Entry {
            id: format!("mock_{}", i),
            sgv: current_glucose,
            direction: Some("Flat".to_string()),
            date_string: Some(t.format("%Y-%m-%dT%H:%M:%S").to_string()),
            mills: Some(t.timestamp_millis() as u64),
        });
    }

    entries
}

#[tokio::main]
async fn main() {
    // utils::graph::draw_graph(&mock_entries(), utils::graph::PrefUnit::MgDl, Some("nightscout_graph.png"));
    // utils::graph::draw_graph(&mock_entries(), utils::graph::PrefUnit::Mmol, Some("nightscout_graph2.png"));

    // let token = dotenvy::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    // let mut client = Client::builder(token, GatewayIntents::empty())
    //     .event_handler(Handler)
    //     .await
    //     .expect("Error creating client");

    // if let Err(why) = client.start().await {
    //     println!("Client error: {why:?}");
    // }

    // Ok(())
}
