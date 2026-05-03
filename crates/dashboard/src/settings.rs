use axum::{
    Form, Router,
    extract::{Query, State},
    http::{HeaderValue, StatusCode, header::HeaderMap},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use beetroot_core::db::TokenUpdate;
use serde::Deserialize;

use crate::{AppState, auth::CurrentUser, middleware::csrf, templates::pages};

/// Mount settings routes.
pub fn router() -> Router<AppState> {
    Router::new().route("/settings", get(show).post(save))
}

/// Optional `?saved=...` flag used to flash a confirmation banner.
#[derive(Deserialize, Default)]
pub struct SettingsQuery {
    pub saved: Option<String>,
}

/// GET /settings
///
/// render the form pre-filled with the user's current values.
async fn show(
    State(state): State<AppState>,
    CurrentUser(session): CurrentUser,
    Query(q): Query<SettingsQuery>,
    headers: HeaderMap,
) -> Response {
    let discord_id = session.discord_id as u64;

    // If they somehow reached /settings without an underlying user row, push
    // them through onboarding instead of crashing.
    let user = match state.db.get_user(discord_id).await {
        Ok(Some(u)) => u,
        Ok(None) => return Redirect::to("/onboarding").into_response(),
        Err(e) => {
            tracing::error!("Failed to load user {discord_id}: {e}");
            return crate::templates::pages::error("Error", "Failed to load your settings.")
                .into_string()
                .pipe(error_html);
        }
    };

    let csrf_token = csrf::token_from_cookies(&headers);
    Html(pages::settings(&session, &user, csrf_token.as_deref(), q.saved.as_deref()).into_string())
        .into_response()
}

/// Form payload from the settings page.
#[derive(Deserialize)]
pub struct SettingsForm {
    pub nightscout_url: String,
    /// One of "keep", "clear", "replace".
    pub token_action: String,
    pub nightscout_token: Option<String>,
    pub is_private: Option<String>,
    pub microbolus_threshold: f64,
    pub display_microbolus: Option<String>,
    pub force_ephemeral: Option<String>,
}

/// POST /settings
///
/// validate and persist the form, then redirect with a flash.
async fn save(
    State(state): State<AppState>,
    CurrentUser(session): CurrentUser,
    Form(form): Form<SettingsForm>,
) -> Response {
    let url = form.nightscout_url.trim();
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return (
            StatusCode::BAD_REQUEST,
            "Nightscout URL must start with https:// or http://",
        )
            .into_response();
    }

    let token_value = form
        .nightscout_token
        .as_deref()
        .map(str::trim)
        .unwrap_or("");

    let token_update = match form.token_action.as_str() {
        "clear" => TokenUpdate::Clear,
        "replace" if !token_value.is_empty() => TokenUpdate::Set(token_value),
        "replace" => {
            return (
                StatusCode::BAD_REQUEST,
                "Token is empty: choose \"Keep\" or \"Remove\", or enter a new value.",
            )
                .into_response();
        }
        _ => TokenUpdate::Keep,
    };

    // Checkboxes are absent when unchecked, present (with value "on") when checked.
    let is_private = form.is_private.as_deref() != Some("false");
    let display_microbolus = form.display_microbolus.is_some();
    let force_ephemeral = form.force_ephemeral.is_some();

    let microbolus_threshold = form.microbolus_threshold.clamp(0.0, 50.0);

    let discord_id = session.discord_id as u64;

    if let Err(e) = state
        .db
        .update_user_settings(
            discord_id,
            url,
            token_update,
            is_private,
            microbolus_threshold,
            display_microbolus,
            force_ephemeral,
        )
        .await
    {
        tracing::error!("Failed to save settings for {discord_id}: {e}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to save. Please try again.",
        )
            .into_response();
    }

    let flash = match form.token_action.as_str() {
        "clear" => "token-cleared",
        "replace" => "token-replaced",
        _ => "general",
    };

    let mut resp = StatusCode::OK.into_response();
    resp.headers_mut().insert(
        "HX-Redirect",
        HeaderValue::from_str(&format!("/settings?saved={flash}")).expect("static-shaped string"),
    );
    resp
}

/// Shape an HTML body string into a full error response (text/html).
fn error_html(body: String) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body,
    )
        .into_response()
}

/// Tiny helper trait to chain a value into a function call without a temp.
trait Pipe: Sized {
    fn pipe<F, R>(self, f: F) -> R
    where
        F: FnOnce(Self) -> R,
    {
        f(self)
    }
}
impl<T> Pipe for T {}
