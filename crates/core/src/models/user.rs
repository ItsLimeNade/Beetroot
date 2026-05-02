use serde::{Deserialize, Serialize};

/// Raw row from the `users` table.
///
/// `nightscout_token` is the **encrypted** ciphertext (AES-256-GCM, base64).
/// `allowed_people` and `blocked_people` are JSON-encoded `Vec<u64>`.
///
/// This type is used only at the DB boundary. Application code should work
/// with [`UserDecrypted`] instead.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub discord_id: i64,
    pub nightscout_url: Option<String>,
    pub nightscout_token: Option<String>,
    pub allowed_people: Option<String>,
    pub blocked_people: Option<String>,
    pub is_private: Option<bool>,
    pub microbolus_threshold: Option<f64>,
    pub display_microbolus: Option<bool>,
    pub force_ephemeral: Option<bool>,
    pub mbg_expiry_time: Option<i64>,
    pub last_seen_version: Option<String>,
}

/// In-memory, fully decrypted view of a user.
///
/// Never persisted directly. Built by the repository layer after decrypting
/// the token and parsing the JSON arrays. The `discord_id` is `u64` here
/// (Discord's native type) the conversion from the DB's `i64` happens at
/// the boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserDecrypted {
    pub discord_id: u64,
    pub nightscout_url: Option<String>,
    pub nightscout_token: Option<String>,
    pub allowed_people: Vec<u64>,
    pub blocked_people: Vec<u64>,
    pub is_private: bool,
    pub microbolus_threshold: f64,
    pub display_microbolus: bool,
    pub force_ephemeral: bool,
    pub mbg_expiry_time: i64,
    pub last_seen_version: Option<String>,
}
