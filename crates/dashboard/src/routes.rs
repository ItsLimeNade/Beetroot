//! Route definitions.

use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};

use crate::AppState;
use crate::auth::MaybeUser;
use crate::middleware::csrf;
use crate::templates::pages;

/// Build the application router.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/", get(index))
}

/// Landing page, shows different content based on login status.
///
/// If the user is logged in but has no row in the `users` table, send them
/// to the onboarding wizard before they see the dashboard.
async fn index(
    State(state): State<AppState>,
    MaybeUser(session): MaybeUser,
    headers: axum::http::HeaderMap,
) -> Response {
    let mut user = None;
    if let Some(s) = &session {
        let discord_id = s.discord_id as u64;
        match state.db.get_user(discord_id).await {
            Ok(Some(u)) => user = Some(u),
            Ok(None) => return Redirect::to("/onboarding").into_response(),
            Err(e) => tracing::error!("Failed to load user {discord_id}: {e}"),
        }
    }

    let csrf_token = csrf::token_from_cookies(&headers);
    Html(pages::index(session.as_ref(), user.as_ref(), csrf_token.as_deref()).into_string())
        .into_response()
}

/// Lightweight liveness check.
async fn health(State(state): State<AppState>) -> StatusCode {
    let ok = sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(state.db.pool())
        .await
        .is_ok();

    if ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}
