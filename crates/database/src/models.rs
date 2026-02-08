use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserData {
    // Nightscout data
    pub nightscout_url: Option<String>,
    pub nightscout_token: Option<String>,

    // Social settings
    pub allowed_people: Vec<u64>,
    pub blocked_people: Vec<u64>,
    pub is_private: bool,

    // User preferences
    pub microbolus_threshold: f32,
    pub display_microbolus: bool,
    pub force_ephemeral: bool,
    pub mbg_expiry_time: u64, // Timestamp in seconds

    // Sticker settings
    pub in_range_stickers: Vec<Sticker>,
    pub low_stickers: Vec<Sticker>,
    pub high_stickers: Vec<Sticker>,
    pub other_stickers: Vec<Sticker>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct Sticker {
    pub sticker_url: String,
    pub display_name: Option<String>,
}

// We need this enum to map database rows to the correct vector in UserData
#[derive(Debug, Clone, Copy, PartialEq, sqlx::Type)]
#[sqlx(rename_all = "snake_case")]
pub enum StickerCategory {
    InRange,
    Low,
    High,
    Other,
}

// Analytics Structs
pub struct Analytics {
    pub commands_analytics: Vec<CommandData>,
    pub usage_analytics: UsageData,
    pub process_analytics: ProcessData,
}

#[derive(Debug, sqlx::FromRow)]
pub struct CommandData {
    pub name: String,
    pub total_use: i64,
    pub weekly_use: i64,
    pub monthly_use: i64,
    pub average_execution_time: i64, // ms
}

pub struct UsageData {
    pub total_users: u64,
    pub daily_active_users: u64,
    pub monthly_active_users: u64,
}

pub struct ProcessData {
    pub uptime: u64, // Seconds
    pub downtime: u64,
    pub bot_version: String,
}
