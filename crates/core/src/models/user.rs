use serde::{Deserialize, Serialize};

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
    pub bg_image_mode: Option<bool>,
    pub active_theme: Option<String>,
    pub treatment_mode: Option<String>,
    pub graph_sticker_count: Option<i64>,
}

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
    pub bg_image_mode: bool,
    pub active_theme: Option<String>,
    pub treatment_mode: String,
    pub graph_sticker_count: i64,
}
