use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use beetroot_core::models::UserDecrypted;
use maud::{Markup, PreEscaped, html};

use crate::{AppState, auth::CurrentUser};

/// The current dashboard/bot version. Bumping this triggers the changelog
/// modal for everyone whose `last_seen_version` is older.
pub const CURRENT_VERSION: &str = "1.0.0";

/// One published version's notes.
pub struct Entry {
    pub version: &'static str,
    pub date: &'static str,
    pub items: &'static [&'static str],
}

/// Released versions, **newest first**. The order matters for
/// [`entries_since`].
pub const ENTRIES: &[Entry] = &[Entry {
    version: "1.0.0",
    date: "2026-05-01",
    items: &["Nothing yet"],
}];

/// Entries the user hasn't seen yet.
///
/// Returns the slice of [`ENTRIES`] from index 0 up to (but not including)
/// the entry whose version matches `last_seen`. If `last_seen` is `None` or
/// doesn't match any known version, returns all entries.
pub fn entries_since(last_seen: Option<&str>) -> &'static [Entry] {
    let Some(last) = last_seen else {
        return ENTRIES;
    };
    match ENTRIES.iter().position(|e| e.version == last) {
        Some(i) => &ENTRIES[..i],
        None => ENTRIES,
    }
}

/// Render the changelog modal. Returns `None` when the user is up to date,
/// so callers can skip the markup entirely.
pub fn modal_for(user: &UserDecrypted) -> Option<Markup> {
    let entries = entries_since(user.last_seen_version.as_deref());
    if entries.is_empty() {
        return None;
    }
    Some(modal(entries))
}

/// Build the modal markup for a non-empty list of entries.
fn modal(entries: &[Entry]) -> Markup {
    html! {
        div #changelog-modal.changelog-modal {
            div.changelog-backdrop {}
            div.changelog-card {
                h2 { "What's new" }
                p.text-muted.mt-1 {
                    "Here's what changed since your last visit."
                }

                div.changelog-entries.mt-2 {
                    @for entry in entries {
                        section.changelog-entry {
                            header.changelog-entry-head {
                                strong { "v" (entry.version) }
                                span.text-muted { (entry.date) }
                            }
                            ul.mt-1 {
                                @for item in entry.items {
                                    li { (item) }
                                }
                            }
                        }
                    }
                }

                div.changelog-actions.mt-2 {
                    button.btn.btn-primary
                        type="button"
                        hx-post="/changelog/seen"
                        hx-target="#changelog-modal"
                        hx-swap="delete"
                    {
                        "Got it"
                    }
                }
            }
            style { (PreEscaped(MODAL_CSS)) }
        }
    }
}

const MODAL_CSS: &str = r#"
.changelog-modal {
    position: fixed;
    inset: 0;
    z-index: 1000;
    display: flex;
    align-items: flex-end;
    justify-content: center;
    padding: 1rem;
}
@media (min-width: 480px) {
    .changelog-modal { align-items: center; }
}
.changelog-backdrop {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.3);
    backdrop-filter: blur(4px);
    -webkit-backdrop-filter: blur(4px);
}
.changelog-card {
    position: relative;
    background: var(--bg-card);
    border-radius: 20px;
    box-shadow: 0 24px 64px rgba(0,0,0,.18);
    padding: 2rem;
    max-width: 480px;
    width: 100%;
    max-height: 75vh;
    overflow-y: auto;
}
.changelog-card h2 {
    font-size: 1.3125rem;
    font-weight: 700;
    letter-spacing: -0.02em;
}
.changelog-entries { display: flex; flex-direction: column; gap: 1.25rem; }
.changelog-entry-head {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.5rem;
}
.changelog-entry-head strong {
    font-size: 0.8125rem;
    font-weight: 600;
    color: var(--accent);
    background: var(--accent-light);
    padding: 0.2rem 0.5rem;
    border-radius: 999px;
}
.changelog-entry-head span {
    font-size: 0.8125rem;
    color: var(--text-muted);
}
.changelog-entry ul {
    padding-left: 1.125rem;
}
.changelog-entry li {
    font-size: 0.9375rem;
    line-height: 1.5;
    margin-bottom: 0.375rem;
    color: var(--text);
}
.changelog-actions {
    display: flex;
    justify-content: flex-end;
    padding-top: 1rem;
    border-top: 1px solid var(--border-light);
}
"#;

/// Mount changelog routes.
pub fn router() -> Router<AppState> {
    Router::new().route("/changelog/seen", post(mark_seen))
}

/// POST /changelog/seen
///
/// record that the user has dismissed the modal.
async fn mark_seen(State(state): State<AppState>, CurrentUser(session): CurrentUser) -> Response {
    let discord_id = session.discord_id as u64;

    if let Err(e) = state
        .db
        .update_user_last_seen_version(discord_id, CURRENT_VERSION)
        .await
    {
        tracing::error!("Failed to update last_seen_version for {discord_id}: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}
