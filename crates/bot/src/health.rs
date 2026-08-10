use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::routing::get;
use axum::{Json, Router};
use beetroot_core::Database;
use poise::serenity_prelude as serenity;
use serde_json::json;

const DEGRADED_GATEWAY: Duration = Duration::from_millis(1000);

#[derive(Clone)]
struct HealthState {
    shard_manager: Arc<serenity::ShardManager>,
    database: Database,
    token: Option<Arc<String>>,
}

pub fn spawn(shard_manager: Arc<serenity::ShardManager>, database: Database) {
    let Some(port) = std::env::var("HEALTH_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
    else {
        tracing::info!("health endpoint disabled (HEALTH_PORT unset)");
        return;
    };

    let token = std::env::var("HEALTH_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
        .map(Arc::new);
    if token.is_none() {
        tracing::warn!(
            "HEALTH_TOKEN unset: /health is unauthenticated — ne l'expose pas publiquement"
        );
    }

    let state = HealthState {
        shard_manager,
        database,
        token,
    };
    let app = Router::new()
        .route("/health", get(health))
        .with_state(state);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    tokio::spawn(async move {
        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => listener,
            Err(e) => {
                tracing::error!(error = %e, %addr, "could not bind health endpoint");
                return;
            }
        };
        tracing::info!(%addr, "health endpoint listening");
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!(error = %e, "health endpoint stopped");
        }
    });
}

async fn health(
    headers: HeaderMap,
    State(state): State<HealthState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Some(expected) = &state.token {
        let provided = headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        if provided != Some(expected.as_str()) {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "status": "UNAUTHORIZED" })),
            );
        }
    }

    let mut problems: Vec<String> = Vec::new();
    let mut gateway_ms: Option<u64> = None;

    {
        let runners = state.shard_manager.runners.lock().await;
        if runners.is_empty() {
            problems.push("no shard running".to_owned());
        }
        for (id, info) in runners.iter() {
            if info.stage != serenity::ConnectionStage::Connected {
                problems.push(format!("shard {} is {:?}", id.get(), info.stage));
            }
            if let Some(latency) = info.latency {
                let ms = latency.as_millis() as u64;
                gateway_ms = Some(gateway_ms.map_or(ms, |worst| worst.max(ms)));
                if latency > DEGRADED_GATEWAY {
                    problems.push(format!("shard {} gateway latency {ms}ms", id.get()));
                }
            }
        }
    }

    let db_start = Instant::now();
    if let Err(e) = state.database.ping().await {
        problems.push(format!("database: {e}"));
    }
    let db_ms = db_start.elapsed().as_millis() as u64;

    let healthy = problems.is_empty();
    let code = if healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        code,
        Json(json!({
            "status": if healthy { "UP" } else { "DOWN" },
            "version": env!("CARGO_PKG_VERSION"),
            "gateway_ms": gateway_ms,
            "database_ms": db_ms,
            "errors": problems,
        })),
    )
}
