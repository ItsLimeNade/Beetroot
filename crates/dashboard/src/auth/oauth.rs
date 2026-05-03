use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use beetroot_core::models::DashboardSession;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::AppState;

// Config

/// OAuth2 settings loaded from environment variables.
struct OAuthConfig {
    client_id: String,
    client_secret: String,
    redirect_uri: String,
}

impl OAuthConfig {
    fn from_env() -> Self {
        Self {
            client_id: std::env::var("DISCORD_CLIENT_ID").expect("Missing DISCORD_CLIENT_ID"),
            client_secret: std::env::var("DISCORD_CLIENT_SECRET")
                .expect("Missing DISCORD_CLIENT_SECRET"),
            redirect_uri: std::env::var("DISCORD_REDIRECT_URI")
                .expect("Missing DISCORD_REDIRECT_URI"),
        }
    }
}

// PKCE helpers

/// Generate a 32-byte random hex string (for state and session tokens).
fn random_hex(len: usize) -> String {
    let bytes: Vec<u8> = (0..len).map(|_| rand::random()).collect();
    hex::encode(&bytes)
}

/// Generate a PKCE code verifier (43-128 chars, URL-safe).
fn generate_code_verifier() -> String {
    let bytes: [u8; 32] = rand::random();
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Derive the S256 code challenge from the verifier.
fn code_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

// Cookie helpers

/// Build a Set-Cookie header value.
fn set_cookie(name: &str, value: &str, max_age_secs: i64, http_only: bool) -> String {
    let mut cookie = format!("{name}={value}; Path=/; SameSite=Lax; Max-Age={max_age_secs}");
    if http_only {
        cookie.push_str("; HttpOnly");
    }
    cookie
}

/// Build a Set-Cookie that expires immediately (delete).
fn delete_cookie(name: &str) -> String {
    format!("{name}=; Path=/; Max-Age=0")
}

/// Extract a cookie value by name from the Cookie header.
fn get_cookie(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    for pair in cookie_header.split(';') {
        let pair = pair.trim();
        if let Some(value) = pair.strip_prefix(&format!("{name}=")) {
            return Some(value.to_string());
        }
    }
    None
}

// Handlers

/// GET /auth/login
///
/// Generates PKCE verifier + state, stores them in a short-lived cookie,
/// and redirects the user to Discord's authorization page.
pub async fn login() -> Response {
    let config = OAuthConfig::from_env();

    let verifier = generate_code_verifier();
    let challenge = code_challenge(&verifier);
    let state = random_hex(16);

    let oauth_cookie_value = format!("{verifier}|{state}");

    let authorize_url = format!(
        "https://discord.com/oauth2/authorize?\
         client_id={client_id}\
         &redirect_uri={redirect_uri}\
         &response_type=code\
         &scope=identify\
         &state={state}\
         &code_challenge={challenge}\
         &code_challenge_method=S256",
        client_id = config.client_id,
        redirect_uri = urlencoding::encode(&config.redirect_uri),
        state = state,
        challenge = challenge,
    );

    let cookie = set_cookie("oauth_pkce", &oauth_cookie_value, 300, true);

    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, authorize_url),
            (header::SET_COOKIE, cookie),
        ],
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct CallbackParams {
    code: String,
    state: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct DiscordUser {
    id: String,
    username: String,
    avatar: Option<String>,
}

/// GET /auth/callback
///
/// Discord redirects here after the user authorizes. We:
/// 1. Validate the `state` parameter against our cookie (CSRF protection)
/// 2. Exchange the authorization code + PKCE verifier for an access token
/// 3. Fetch the user's Discord profile
/// 4. Create a session in the DB
/// 5. Set a session cookie and redirect to /
pub async fn callback(
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
    headers: axum::http::HeaderMap,
) -> Response {
    let cookie_value = match get_cookie(&headers, "oauth_pkce") {
        Some(v) => v,
        None => return auth_error("Missing OAuth cookie. Please try logging in again."),
    };

    let (verifier, expected_state) = match cookie_value.split_once('|') {
        Some(pair) => pair,
        None => return auth_error("Malformed OAuth cookie."),
    };

    if params.state != expected_state {
        return auth_error("State mismatch — possible CSRF attack. Please try again.");
    }

    let config = OAuthConfig::from_env();

    let client = reqwest::Client::new();

    let form_body = format!(
        "client_id={}&client_secret={}&grant_type=authorization_code&code={}&redirect_uri={}&code_verifier={}",
        urlencoding::encode(&config.client_id),
        urlencoding::encode(&config.client_secret),
        urlencoding::encode(&params.code),
        urlencoding::encode(&config.redirect_uri),
        urlencoding::encode(verifier),
    );

    let token_res = client
        .post("https://discord.com/api/v10/oauth2/token")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form_body)
        .send()
        .await;

    let token_res = match token_res {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            tracing::error!("Discord token exchange failed: {status} — {body}");
            return auth_error("Failed to exchange authorization code. Please try again.");
        }
        Err(e) => {
            tracing::error!("Discord token exchange request error: {e}");
            return auth_error("Could not reach Discord. Please try again.");
        }
    };

    let token: TokenResponse = match token_res.json().await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to parse token response: {e}");
            return auth_error("Unexpected response from Discord.");
        }
    };

    let user_res = client
        .get("https://discord.com/api/v10/users/@me")
        .header("Authorization", format!("Bearer {}", token.access_token))
        .send()
        .await;

    let discord_user: DiscordUser = match user_res {
        Ok(r) if r.status().is_success() => match r.json().await {
            Ok(u) => u,
            Err(e) => {
                tracing::error!("Failed to parse Discord user: {e}");
                return auth_error("Could not read your Discord profile.");
            }
        },
        Ok(r) => {
            tracing::error!("Discord /users/@me failed: {}", r.status());
            return auth_error("Failed to fetch your Discord profile.");
        }
        Err(e) => {
            tracing::error!("Discord /users/@me request error: {e}");
            return auth_error("Could not reach Discord.");
        }
    };

    let discord_id: i64 = match discord_user.id.parse() {
        Ok(id) => id,
        Err(_) => return auth_error("Invalid Discord user ID."),
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock")
        .as_secs() as i64;

    let session_id = random_hex(32); // 256-bit token
    let session = DashboardSession {
        id: session_id.clone(),
        discord_id,
        discord_username: discord_user.username,
        discord_avatar: discord_user.avatar,
        created_at: now,
        last_active_at: now,
        expires_at: now + 7 * 24 * 3600, // 7 days
    };

    if let Err(e) = state.db.create_session(&session).await {
        tracing::error!("Failed to create session: {e}");
        return auth_error("Internal error creating your session.");
    }

    let mut headers = HeaderMap::new();
    headers.insert(header::LOCATION, HeaderValue::from_static("/"));
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&set_cookie("session", &session_id, 7 * 24 * 3600, true))
            .expect("valid cookie"),
    );
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&delete_cookie("oauth_pkce")).expect("valid cookie"),
    );

    (StatusCode::SEE_OTHER, headers).into_response()
}

/// GET /auth/logout
///
/// Deletes the session from DB and clears the cookie.
pub async fn logout(State(state): State<AppState>, headers: axum::http::HeaderMap) -> Response {
    if let Some(session_id) = get_cookie(&headers, "session") {
        let _ = state.db.delete_session(&session_id).await;
    }

    let clear = delete_cookie("session");

    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, "/".to_string()),
            (header::SET_COOKIE, clear),
        ],
    )
        .into_response()
}

// Helpers

/// Render a styled HTML error page for auth failures.
fn auth_error(msg: &str) -> Response {
    let body = crate::templates::pages::error("Authentication Failed", msg).into_string();
    (
        StatusCode::BAD_REQUEST,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body,
    )
        .into_response()
}
