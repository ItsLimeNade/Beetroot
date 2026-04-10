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

/// A full sticker row from the DB, including id and category.
/// Used for flat sticker operations (generation, CRUD).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StickerRow {
    pub id: i64,
    pub sticker_url: String,
    pub display_name: Option<String>,
    pub category: StickerCategory,
}

// We need this enum to map database rows to the correct vector in UserData
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StickerCategory {
    InRange,
    Low,
    High,
    Other,
}

impl StickerCategory {
    /// Parse a category from its DB string representation.
    pub fn from_db_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "in_range" | "inrange" | "in range" => Some(Self::InRange),
            "low" => Some(Self::Low),
            "high" => Some(Self::High),
            "other" => Some(Self::Other),
            _ => None,
        }
    }

    /// The string stored in the DB `category` column.
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::InRange => "in_range",
            Self::Low => "low",
            Self::High => "high",
            Self::Other => "other",
        }
    }

    /// Human-readable name for Discord embeds.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::InRange => "In Range",
            Self::Low => "Low",
            Self::High => "High",
            Self::Other => "Any / No Context",
        }
    }

    /// Maximum number of stickers a user can have per category.
    pub fn max_count(self) -> i64 {
        match self {
            Self::Low => 3,
            Self::InRange => 3,
            Self::High => 3,
            Self::Other => 5,
        }
    }

    /// Whether this category is contextual (tied to a glucose state).
    pub fn is_contextual(self) -> bool {
        !matches!(self, Self::Other)
    }

    /// All contextual categories.
    pub fn contextual_variants() -> &'static [StickerCategory] {
        &[Self::Low, Self::InRange, Self::High]
    }

    /// All categories including Other.
    pub fn all_variants() -> &'static [StickerCategory] {
        &[Self::Low, Self::InRange, Self::High, Self::Other]
    }
}

impl std::fmt::Display for StickerCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
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
