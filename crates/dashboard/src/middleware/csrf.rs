//! CSRF token generation and validation.
//!
//! Each form includes a hidden `<input name="_csrf" value="...">`.
//! On POST, the middleware compares it against the value in the session cookie.
//!
//! The token is a 32-byte random hex string, stored in a `csrf_token` cookie
//! (HttpOnly, SameSite=Lax). It's generated once per session and reused until
//! the session ends.

use axum::{
    extract::Request,
    http::{Method, StatusCode, header, header::HeaderValue},
    middleware::Next,
    response::{IntoResponse, Response},
};

/// Generate a new CSRF token.
pub fn generate_token() -> String {
    let bytes: [u8; 32] = rand::random();
    hex::encode(bytes)
}

/// Extract the CSRF token from the cookie header.
pub fn token_from_cookies(headers: &axum::http::HeaderMap) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie_header
        .split(';')
        .map(|s| s.trim())
        .find_map(|pair| pair.strip_prefix("csrf_token="))
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
}

/// Middleware that validates the CSRF token on state-changing requests.
///
/// For GET/HEAD/OPTIONS, it passes through (and sets the cookie if missing).
/// For POST/PUT/PATCH/DELETE, it checks that the form field `_csrf` matches
/// the cookie value.
pub async fn csrf_protection(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let is_safe = matches!(method, Method::GET | Method::HEAD | Method::OPTIONS);

    let incoming_token = token_from_cookies(request.headers());

    if !is_safe {
        let header_token = request
            .headers()
            .get("X-CSRF-Token")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        match (&incoming_token, header_token) {
            (Some(cookie), Some(header)) if *cookie == header => {}
            _ => {
                return (StatusCode::FORBIDDEN, "CSRF token mismatch").into_response();
            }
        }
    }

    let mut response = next.run(request).await;

    // Issue a CSRF cookie only on the very first visit (no cookie yet).
    // Once established, the same value is reused. Rotating it would make the
    // meta tag baked into the current page stale, causing the next POST to 403.
    if incoming_token.is_none() {
        let token = generate_token();
        let cookie = format!(
            "csrf_token={token}; Path=/; SameSite=Lax; HttpOnly; Max-Age={}",
            7 * 24 * 3600,
        );
        if let Ok(val) = HeaderValue::from_str(&cookie) {
            response.headers_mut().append(header::SET_COOKIE, val);
        }
    }

    response
}
