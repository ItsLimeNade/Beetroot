use serde::{Deserialize, Serialize};

/// A sticker row from the `stickers` table.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct Sticker {
    pub id: i64,
    pub discord_id: i64,
    pub sticker_url: String,
    pub display_name: Option<String>,
    pub category: StickerCategory,
}

/// Glucose-state category that triggers a sticker.
///
/// Stored in the DB and serialized over the wire as lowercase snake_case
/// (e.g. `"in_range"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum StickerCategory {
    InRange,
    Low,
    High,
    Other,
}

impl StickerCategory {
    /// Human-readable name for Discord embeds and the dashboard.
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
            Self::Low | Self::InRange | Self::High => 3,
            Self::Other => 5,
        }
    }

    /// Whether this category is contextual (tied to a glucose state).
    pub fn is_contextual(self) -> bool {
        !matches!(self, Self::Other)
    }

    /// Contextual variants only (Low, InRange, High).
    pub fn contextual_variants() -> &'static [StickerCategory] {
        &[Self::Low, Self::InRange, Self::High]
    }

    /// All four variants.
    pub fn all_variants() -> &'static [StickerCategory] {
        &[Self::Low, Self::InRange, Self::High, Self::Other]
    }
}

impl std::fmt::Display for StickerCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}
