mod extractors;
pub mod oauth;

pub use extractors::{CurrentUser, MaybeUser};

use axum::{Router, routing::get};

use crate::AppState;

/// Mount authentication routes.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/login", get(oauth::login))
        .route("/auth/callback", get(oauth::callback))
        .route("/auth/logout", get(oauth::logout))
}
