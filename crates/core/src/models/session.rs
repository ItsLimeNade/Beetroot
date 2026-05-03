use serde::{Deserialize, Serialize};

/// A dashboard authentication session.
///
/// `id` is a 256-bit cryptographically random token, stored as a hex string.
/// All timestamps are Unix epoch seconds.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct DashboardSession {
    pub id: String,
    pub discord_id: i64,
    pub discord_username: String,
    pub discord_avatar: Option<String>,
    pub created_at: i64,
    pub last_active_at: i64,
    pub expires_at: i64,
}
