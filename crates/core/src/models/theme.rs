use serde::{Deserialize, Serialize};

/// A custom theme row from the `themes` table.
///
/// `data` holds a JSON object of all bonbon `Theme` color fields as hex
/// strings. The bot crate (which depends on bonbon) is responsible for
/// turning this into an actual `bonbon::theme::Theme`; core stays
/// rendering-agnostic.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct ThemeRow {
    pub id: i64,
    pub discord_id: i64,
    pub name: String,
    pub data: String,
}
