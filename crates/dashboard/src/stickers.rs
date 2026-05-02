use axum::{
    Form, Router,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header::HeaderMap},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use beetroot_core::models::sticker::StickerCategory;
use serde::Deserialize;

use crate::{AppState, auth::CurrentUser, middleware::csrf, templates::pages};

const MAX_URL_LEN: usize = 2048;
const MAX_NAME_LEN: usize = 64;

/// Mount sticker routes.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/stickers", get(show).post(add))
        .route("/stickers/{id}/delete", post(delete))
}

/// GET /stickers
///
/// List all stickers for the user, grouped by category.
async fn show(
    State(state): State<AppState>,
    CurrentUser(session): CurrentUser,
    headers: HeaderMap,
) -> Response {
    let discord_id = session.discord_id as u64;

    // Same safety net as /settings — push to onboarding if no user row.
    let user = match state.db.get_user(discord_id).await {
        Ok(Some(u)) => u,
        Ok(None) => return Redirect::to("/onboarding").into_response(),
        Err(e) => {
            tracing::error!("Failed to load user {discord_id}: {e}");
            return internal_error("Failed to load your profile.");
        }
    };

    let stickers = match state.db.get_all_user_stickers(discord_id).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to load stickers for {discord_id}: {e}");
            return internal_error("Failed to load your stickers.");
        }
    };

    let csrf_token = csrf::token_from_cookies(&headers);
    Html(pages::stickers(&session, &user, &stickers, csrf_token.as_deref()).into_string())
        .into_response()
}

/// Form payload for the "add sticker" form.
#[derive(Deserialize)]
pub struct AddStickerForm {
    pub category: StickerCategory,
    pub sticker_url: String,
    pub display_name: Option<String>,
}

/// POST /stickers
///
/// Validate and insert a new sticker.
async fn add(
    State(state): State<AppState>,
    CurrentUser(session): CurrentUser,
    Form(form): Form<AddStickerForm>,
) -> Response {
    let discord_id = session.discord_id as u64;

    let url = form.sticker_url.trim();
    if !url.starts_with("https://") {
        return bad_request("URL must start with https://.");
    }
    if url.len() > MAX_URL_LEN {
        return bad_request("URL too long.");
    }

    let display_name = form
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    if let Some(n) = display_name
        && n.len() > MAX_NAME_LEN
    {
        return bad_request("Name too long (max 64 characters).");
    }

    match state
        .db
        .get_sticker_count_by_category(discord_id, form.category)
        .await
    {
        Ok(count) if count >= form.category.max_count() => {
            return bad_request(&format!(
                "You've reached the limit of {} stickers for {}.",
                form.category.max_count(),
                form.category.display_name()
            ));
        }
        Err(e) => {
            tracing::error!("Failed to count stickers: {e}");
            return internal_error("Verification error.");
        }
        _ => {}
    }

    // Reject duplicates of the same URL.
    match state.db.sticker_url_exists(discord_id, url).await {
        Ok(true) => return bad_request("You already have a sticker with this URL."),
        Err(e) => {
            tracing::error!("Failed to check sticker dup: {e}");
            return internal_error("Verification error.");
        }
        _ => {}
    }

    if let Err(e) = state
        .db
        .insert_sticker(discord_id, url, display_name.unwrap_or(""), form.category)
        .await
    {
        tracing::error!("Failed to insert sticker for {discord_id}: {e}");
        return internal_error("Failed to save.");
    }

    refresh_stickers()
}

/// POST /stickers/:id/delete
///
/// remove a sticker.
async fn delete(
    State(state): State<AppState>,
    CurrentUser(session): CurrentUser,
    Path(sticker_id): Path<i64>,
) -> Response {
    let discord_id = session.discord_id as u64;

    match state.db.delete_user_sticker(discord_id, sticker_id).await {
        Ok(true) => refresh_stickers(),
        Ok(false) => (StatusCode::NOT_FOUND, "Sticker not found.").into_response(),
        Err(e) => {
            tracing::error!("Failed to delete sticker {sticker_id}: {e}");
            internal_error("Failed to delete.")
        }
    }
}

/// htmx response that triggers a full page reload of /stickers.
fn refresh_stickers() -> Response {
    let mut resp = StatusCode::OK.into_response();
    resp.headers_mut()
        .insert("HX-Refresh", HeaderValue::from_static("true"));
    resp
}

fn bad_request(msg: &str) -> Response {
    (StatusCode::BAD_REQUEST, msg.to_string()).into_response()
}

fn internal_error(msg: &str) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, msg.to_string()).into_response()
}
