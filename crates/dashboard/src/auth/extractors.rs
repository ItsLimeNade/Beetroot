use axum::{
    extract::FromRequestParts,
    http::{header, request::Parts},
    response::{IntoResponse, Redirect, Response},
};
use beetroot_core::models::DashboardSession;

use crate::AppState;

/// Extract the session cookie value from the request headers.
fn session_cookie(parts: &Parts) -> Option<String> {
    let cookie_header = parts.headers.get(header::COOKIE)?.to_str().ok()?;

    cookie_header
        .split(';')
        .map(|s| s.trim())
        .find_map(|pair| pair.strip_prefix("session="))
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
}

async fn resolve_session(parts: &Parts, state: &AppState) -> Option<DashboardSession> {
    let session_id = session_cookie(parts)?;

    let session = state.db.get_session(&session_id).await.ok()??;

    let _ = state.db.touch_session(&session_id).await;

    Some(session)
}

/// Extractor that **requires** an authenticated session.
///
/// If the user is not logged in, they are redirected to `/auth/login`.
pub struct CurrentUser(pub DashboardSession);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match resolve_session(parts, state).await {
            Some(session) => Ok(CurrentUser(session)),
            None => Err(Redirect::to("/auth/login").into_response()),
        }
    }
}

/// Extractor that optionally resolves the current session.
///
/// Always succeeds, returns `None` if the user is not logged in.
pub struct MaybeUser(pub Option<DashboardSession>);

impl FromRequestParts<AppState> for MaybeUser {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(MaybeUser(resolve_session(parts, state).await))
    }
}
