mod auth;
mod changelog;
mod middleware;
mod onboarding;
mod routes;
mod settings;
mod stickers;
mod templates;

use axum::Router;
use beetroot_core::Database;
use std::env;
use tower_http::trace::TraceLayer;

/// Shared application state, available in every request handler via axum's
#[derive(Clone)]
pub struct AppState {
    pub db: Database,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("beetroot_dashboard=debug,info")
            }),
        )
        .init();

    let db_url = env::var("DATABASE_URL").expect("Missing DATABASE_URL");
    let db = Database::connect(&db_url).await?;

    tracing::info!("Connected to database");

    let state = AppState { db };

    let app = Router::new()
        .merge(routes::router())
        .merge(auth::router())
        .merge(onboarding::router())
        .merge(settings::router())
        .merge(stickers::router())
        .merge(changelog::router())
        .layer(axum::middleware::from_fn(middleware::csrf::csrf_protection))
        .layer(axum::middleware::from_fn(
            middleware::security::security_headers,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let bind_addr = env::var("DASHBOARD_BIND").unwrap_or_else(|_| "127.0.0.1:3000".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;

    tracing::info!("Dashboard listening on http://{bind_addr}");

    axum::serve(listener, app).await?;

    Ok(())
}
