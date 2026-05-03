//! Security headers middleware.
//!
//! Adds standard protective headers to every response:
//! - `X-Content-Type-Options: nosniff` -> prevents MIME-type sniffing
//! - `X-Frame-Options: DENY` -> blocks embedding in iframes (clickjacking)
//! - `Referrer-Policy: strict-origin-when-cross-origin` -> limits referrer leakage
//! - `Content-Security-Policy` -> restricts resource loading to same origin
//! - `X-XSS-Protection: 0` -> disables the legacy XSS auditor (it can cause issues)
//! - `Permissions-Policy` -> disables unneeded browser features

use axum::{extract::Request, http::header::HeaderValue, middleware::Next, response::Response};

pub async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    headers.insert(
        "X-Content-Type-Options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("X-Frame-Options", HeaderValue::from_static("DENY"));
    headers.insert(
        "Referrer-Policy",
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(
        "Content-Security-Policy",
        HeaderValue::from_static(
            "default-src 'self'; \
             script-src 'self' 'unsafe-inline' https://unpkg.com; \
             style-src 'self' 'unsafe-inline'; \
             img-src 'self' https: data:; \
             frame-ancestors 'none'",
        ),
    );
    headers.insert("X-XSS-Protection", HeaderValue::from_static("0"));
    headers.insert(
        "Permissions-Policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );

    response
}
