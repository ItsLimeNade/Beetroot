mod commands;
mod data;
mod events;

use anyhow::Context as _;
use poise::serenity_prelude as serenity;
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("beetroot=debug,info")),
        )
        .init();

    tracing::info!("[INIT] Starting Beetroot (Poise Refactor)");

    let options = poise::FrameworkOptions {
        commands: vec![commands::bg::bg(), commands::setup::setup()],

        event_handler: |ctx, event, framework, data| {
            Box::pin(events::event_handler(ctx, event, framework, data))
        },
        on_error: |error| Box::pin(events::on_error(error)),
        post_command: |ctx| Box::pin(events::post_command(ctx)),

        ..Default::default()
    };

    let framework = poise::Framework::builder()
        .options(options)
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                tracing::info!("[CMD] Slash commands registered");

                let db_url = env::var("DATABASE_URL").expect("Missing DATABASE_URL");
                let database = database::Database::new(&db_url).await?;

                Ok(data::Data { database })
            })
        })
        .build();

    let token = env::var("DISCORD_TOKEN").context("Missing DISCORD_TOKEN")?;

    let intents = serenity::GatewayIntents::non_privileged();

    let mut client = serenity::Client::builder(token, intents)
        .framework(framework)
        .await?;

    client.start().await?;
    Ok(())
}
