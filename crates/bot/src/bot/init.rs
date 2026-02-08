use crate::bot::Handler;
use anyhow::Result;
use serenity::prelude::*;

/// Initialize and start the Discord bot
pub async fn start_bot() -> Result<()> {
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
