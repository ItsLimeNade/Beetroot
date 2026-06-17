#[macro_use]
mod macros;

mod changelog;
mod commands;
mod data;
mod events;
mod logging;
mod tips;
mod utils;

use anyhow::Context as _;
use poise::serenity_prelude as serenity;
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // Keep the appender guards alive for the whole run so file logs are flushed.
    let _log_guards = logging::init();

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting Beetroot");
    if logging::sensitive() {
        tracing::warn!(
            "LOG_SENSITIVE is ON: logs will include Discord identifiers and raw \
             medical-data dumps. Do NOT enable this on a public/shared deployment."
        );
    }

    // Fail closed: refuse to boot if token encryption isn't configured with a
    // real secret, rather than silently deriving a key from public source code.
    beetroot_core::crypto::init().context("token encryption is not configured")?;

    let options = poise::FrameworkOptions {
        commands: vec![
            commands::bg::bg(),
            commands::setup::setup(),
            commands::graph::graph(),
            commands::tir::tir(),
            commands::a1c::a1c(),
            commands::add_sticker::add_sticker(),
            commands::add_sticker::add_sticker_context(),
            commands::stickers::stickers(),
            commands::theme::theme(),
            commands::nutrition::nutrition(),
            commands::info::info(),
            commands::changelog::changelog(),
            commands::settings::settings(),
            commands::privacy::privacy(),
            commands::allow::allow(),
            commands::unallow::unallow(),
            commands::block::block(),
            commands::unblock::unblock(),
            commands::microbolus::microbolus(),
            commands::ephemeral::ephemeral(),
            commands::image_mode::image_mode(),
            commands::fingerprick_expiry::fingerprick_expiry(),
            commands::url::url(),
            commands::token::token(),
            commands::delete_account::delete_account(),
        ],

        event_handler: |ctx, event, framework, data| {
            Box::pin(events::event_handler(ctx, event, framework, data))
        },
        on_error: |error| Box::pin(events::on_error(error)),
        pre_command: |ctx| Box::pin(events::pre_command(ctx)),
        post_command: |ctx| Box::pin(events::post_command(ctx)),

        ..Default::default()
    };

    let framework = poise::Framework::builder()
        .options(options)
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                tracing::info!(
                    count = framework.options().commands.len(),
                    "slash commands registered"
                );

                // Resolve emoji ids from this application's own emojis so the same
                // binary works as either the beta or the production bot. Falls back
                // to the ids baked into utils::emojis if the fetch fails.
                match ctx.http.get_application_emojis().await {
                    Ok(list) => {
                        let present: std::collections::HashSet<&str> =
                            list.iter().map(|e| e.name.as_str()).collect();
                        let missing: Vec<&str> = utils::emojis::NAMES
                            .iter()
                            .copied()
                            .filter(|n| !present.contains(n))
                            .collect();
                        if !missing.is_empty() {
                            tracing::warn!(
                                ?missing,
                                "application is missing expected emojis; those will use fallback ids"
                            );
                        }
                        let count = list.len();
                        utils::emojis::init(
                            list.into_iter().map(|e| (e.name, e.id.get(), e.animated)),
                        );
                        tracing::info!(count, "loaded application emojis");
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "could not load application emojis; using built-in fallback ids"
                        );
                    }
                }

                let db_url = env::var("DATABASE_URL").context("Missing DATABASE_URL")?;
                let database = beetroot_core::Database::connect(&db_url).await?;
                tracing::info!("database connected and migrated");

                Ok(data::Data { database })
            })
        })
        .build();

    let token = env::var("DISCORD_TOKEN").context("Missing DISCORD_TOKEN")?;

    let intents = serenity::GatewayIntents::non_privileged();

    let mut client = serenity::Client::builder(token, intents)
        .framework(framework)
        .await?;

    // Shut the gateway down cleanly on Ctrl-C / SIGTERM (e.g. `docker stop`) so
    // `client.start()` returns and the log-appender guards flush on the way out.
    let shard_manager = client.shard_manager.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        tracing::info!("shutdown signal received; stopping gateway");
        shard_manager.shutdown_all().await;
    });

    tracing::info!("connecting to Discord gateway");
    match client.start().await {
        Ok(()) => {
            tracing::info!("client stopped cleanly");
            Ok(())
        }
        Err(e) => {
            tracing::error!(error = %e, "client stopped with error");
            Err(e.into())
        }
    }
}

/// Resolves when the process is asked to stop: Ctrl-C on any platform, plus
/// SIGTERM on Unix (the signal `docker stop` and most orchestrators send).
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(e) => {
                tracing::warn!(error = %e, "could not install SIGTERM handler");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
