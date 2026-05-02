use axum::{
    Form, Router,
    extract::State,
    http::{HeaderValue, StatusCode, header::HeaderMap},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use serde::Deserialize;

use crate::{AppState, auth::CurrentUser, middleware::csrf, templates::pages};

/// Mount onboarding routes.
pub fn router() -> Router<AppState> {
    Router::new().route("/onboarding", get(show).post(submit))
}

/// GET /onboarding
///
/// render the wizard.
///
/// If the user is already registered, send them to the dashboard.
async fn show(
    State(state): State<AppState>,
    CurrentUser(session): CurrentUser,
    headers: HeaderMap,
) -> Response {
    let discord_id = session.discord_id as u64;

    if state.db.user_exists(discord_id).await.unwrap_or(false) {
        return Redirect::to("/").into_response();
    }

    let csrf_token = csrf::token_from_cookies(&headers);
    Html(pages::onboarding(&session, csrf_token.as_deref()).into_string()).into_response()
}

/// Form payload posted by the onboarding wizard.
#[derive(Deserialize)]
pub struct OnboardingForm {
    pub nightscout_url: String,
    pub nightscout_token: Option<String>,
    pub is_private: Option<String>,
}

/// POST /onboarding
///
///  persist the user's settings, then redirect.
async fn submit(
    State(state): State<AppState>,
    CurrentUser(session): CurrentUser,
    Form(form): Form<OnboardingForm>,
) -> Response {
    let url = form.nightscout_url.trim();
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return (
            StatusCode::BAD_REQUEST,
            "Nightscout URL must start with https:// or http://",
        )
            .into_response();
    }

    let token = form
        .nightscout_token
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    // Radio defaults to "true" so missing field equals private.
    let is_private = form.is_private.as_deref() != Some("false");

    let discord_id = session.discord_id as u64;

    if let Err(e) = state
        .db
        .update_user_nightscout(discord_id, url, token, is_private)
        .await
    {
        tracing::error!("Failed to save onboarding for {discord_id}: {e}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to save. Please try again.",
        )
            .into_response();
    }

    let mut resp = StatusCode::OK.into_response();
    resp.headers_mut()
        .insert("HX-Redirect", HeaderValue::from_static("/"));
    resp
}
