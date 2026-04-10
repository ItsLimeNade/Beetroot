mod crypto;
pub mod models;

use crate::models::{Analytics, CommandData, ProcessData, Sticker, UsageData, UserData};
use crate::models::{StickerCategory, StickerRow};

use anyhow::Result;
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
    start_time: u64,
}

impl Database {
    pub async fn new(database_url: &str) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(database_url)?.create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await?;

        sqlx::migrate!().run(&pool).await?;

        Ok(Self {
            pool,
            start_time: current_time(),
        })
    }

    pub async fn get_user_data(&self, user_id: u64) -> Result<Option<UserData>> {
        let id = user_id as i64;

        let user = sqlx::query!(r#"SELECT * FROM users WHERE discord_id = ?"#, id)
            .fetch_optional(&self.pool)
            .await?;

        let user = match user {
            Some(u) => u,
            None => return Ok(None),
        };

        let sticker_rows = sqlx::query!(
            r#"SELECT sticker_url, display_name, category FROM stickers WHERE discord_id = ?"#,
            id
        )
        .fetch_all(&self.pool)
        .await?;

        let mut in_range = Vec::new();
        let mut low = Vec::new();
        let mut high = Vec::new();
        let mut other = Vec::new();

        for row in sticker_rows {
            let sticker = Sticker {
                sticker_url: row.sticker_url,
                display_name: row.display_name,
            };

            match row.category.as_str() {
                "in_range" => in_range.push(sticker),
                "low" => low.push(sticker),
                "high" => high.push(sticker),
                _ => other.push(sticker),
            }
        }

        let nightscout_token = user
            .nightscout_token
            .as_ref()
            .and_then(|t| crypto::decrypt_token(t.as_str()).ok());

        let allowed_people: Vec<u64> =
            serde_json::from_str(&user.allowed_people.unwrap_or_else(|| "[]".into()))?;
        let blocked_people: Vec<u64> =
            serde_json::from_str(&user.blocked_people.unwrap_or_else(|| "[]".into()))?;

        Ok(Some(UserData {
            nightscout_url: user.nightscout_url,
            nightscout_token,
            allowed_people,
            blocked_people,
            is_private: user.is_private.unwrap_or(1) != 0,
            microbolus_threshold: user.microbolus_threshold.unwrap_or(0.5) as f32,
            display_microbolus: user.display_microbolus.unwrap_or(1) != 0,
            force_ephemeral: user.force_ephemeral.unwrap_or(0) != 0,
            mbg_expiry_time: user.mbg_expiry_time.unwrap_or(900) as u64,
            in_range_stickers: in_range,
            low_stickers: low,
            high_stickers: high,
            other_stickers: other,
        }))
    }

    pub async fn log_command_execution(
        &self,
        command: &str,
        user_id: u64,
        duration_ms: u64,
    ) -> Result<()> {
        let now = current_time() as i64;
        let us = user_id as i64;
        let dur = duration_ms as i64;
        sqlx::query!(
            "INSERT INTO command_logs (command_name, user_id, execution_time_ms, created_at) VALUES (?, ?, ?, ?)",
            command, us, dur, now
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_analytics(&self) -> Result<Analytics> {
        let now = current_time() as i64;
        let week_ago = now - 604800;
        let month_ago = now - 2592000;

        let commands_analytics = sqlx::query_as!(
            CommandData,
            r#"
            SELECT 
                command_name as name,
                COUNT(*) as total_use,
                SUM(CASE WHEN created_at > ? THEN 1 ELSE 0 END) as weekly_use,
                SUM(CASE WHEN created_at > ? THEN 1 ELSE 0 END) as monthly_use,
                CAST(AVG(execution_time_ms) AS INTEGER) as average_execution_time
            FROM command_logs
            GROUP BY command_name
            "#,
            week_ago,
            month_ago
        )
        .fetch_all(&self.pool)
        .await?;

        let total_users: i64 = sqlx::query!("SELECT COUNT(*) as c FROM users")
            .fetch_one(&self.pool)
            .await?
            .c;
        let val = now - 86400;
        let daily_active: i64 = sqlx::query!(
            "SELECT COUNT(DISTINCT user_id) as c FROM command_logs WHERE created_at > ?",
            val,
        )
        .fetch_one(&self.pool)
        .await?
        .c;

        let monthly_active: i64 = sqlx::query!(
            "SELECT COUNT(DISTINCT user_id) as c FROM command_logs WHERE created_at > ?",
            month_ago
        )
        .fetch_one(&self.pool)
        .await?
        .c;

        let uptime = current_time() - self.start_time;
        let downtime = 0;
        let bot_version = env!("CARGO_PKG_VERSION").to_string();

        Ok(Analytics {
            commands_analytics,
            usage_analytics: UsageData {
                total_users: total_users as u64,
                daily_active_users: daily_active as u64,
                monthly_active_users: monthly_active as u64,
            },
            process_analytics: ProcessData {
                uptime,
                downtime,
                bot_version,
            },
        })
    }

    pub async fn update_user_nightscout(
        &self,
        user_id: u64,
        url: &str,
        token: Option<&str>,
        is_private: bool,
    ) -> Result<()> {
        let id = user_id as i64;

        // Encrypt token if present
        let encrypted_token = if let Some(t) = token {
            if t.trim().is_empty() {
                None
            } else {
                Some(crypto::encrypt_token(t)?)
            }
        } else {
            None
        };

        sqlx::query!(
            r#"
            INSERT INTO users (discord_id, nightscout_url, nightscout_token, is_private)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(discord_id) DO UPDATE SET
                nightscout_url = excluded.nightscout_url,
                nightscout_token = excluded.nightscout_token,
                is_private = excluded.is_private
            "#,
            id,
            url,
            encrypted_token,
            is_private
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Insert a new sticker for a user.
    pub async fn insert_sticker(
        &self,
        user_id: u64,
        sticker_url: &str,
        display_name: &str,
        category: StickerCategory,
    ) -> Result<()> {
        let id = user_id as i64;
        let cat = category.as_db_str();

        sqlx::query!(
            r#"INSERT INTO stickers (discord_id, sticker_url, display_name, category)
            VALUES (?, ?, ?, ?)"#,
            id,
            sticker_url,
            display_name,
            cat,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Delete a sticker by its row ID.
    pub async fn delete_sticker(&self, sticker_id: i64) -> Result<()> {
        sqlx::query!(r#"DELETE FROM stickers WHERE id = ?"#, sticker_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Delete all stickers for a user.
    pub async fn clear_user_stickers(&self, user_id: u64) -> Result<()> {
        let id = user_id as i64;
        sqlx::query!(r#"DELETE FROM stickers WHERE discord_id = ?"#, id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Count how many stickers a user has in a specific category.
    pub async fn get_sticker_count_by_category(
        &self,
        user_id: u64,
        category: StickerCategory,
    ) -> Result<i64> {
        let id = user_id as i64;
        let cat = category.as_db_str();

        let row = sqlx::query!(
            r#"SELECT COUNT(*) as "count: i64" FROM stickers
            WHERE discord_id = ? AND category = ?"#,
            id,
            cat,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(row.count)
    }

    /// Get all stickers for a user as flat StickerRow (with id + category).
    /// Useful for sticker generation where we need to pick from all categories.
    pub async fn get_all_user_stickers(&self, user_id: u64) -> Result<Vec<StickerRow>> {
        let id = user_id as i64;

        let rows = sqlx::query!(
            r#"SELECT id, sticker_url, display_name, category
            FROM stickers WHERE discord_id = ?"#,
            id,
        )
        .fetch_all(&self.pool)
        .await?;

        let stickers = rows
            .into_iter()
            .filter_map(|row| {
                let category = StickerCategory::from_db_str(&row.category)?;
                Some(StickerRow {
                    id: row.id,
                    sticker_url: row.sticker_url,
                    display_name: row.display_name,
                    category,
                })
            })
            .collect();

        Ok(stickers)
    }

    /// Get stickers for a user filtered by category, as flat StickerRow.
    pub async fn get_user_stickers_by_category(
        &self,
        user_id: u64,
        category: StickerCategory,
    ) -> Result<Vec<StickerRow>> {
        let id = user_id as i64;
        let cat = category.as_db_str();

        let rows = sqlx::query!(
            r#"SELECT id, sticker_url, display_name, category
            FROM stickers WHERE discord_id = ? AND category = ?"#,
            id,
            cat,
        )
        .fetch_all(&self.pool)
        .await?;

        let stickers = rows
            .into_iter()
            .filter_map(|row| {
                let category = StickerCategory::from_db_str(&row.category)?;
                Some(StickerRow {
                    id: row.id,
                    sticker_url: row.sticker_url,
                    display_name: row.display_name,
                    category,
                })
            })
            .collect();

        Ok(stickers)
    }

    /// Check if a user already has a sticker with the given URL.
    pub async fn sticker_url_exists(&self, user_id: u64, sticker_url: &str) -> Result<bool> {
        let id = user_id as i64;

        let row = sqlx::query!(
            r#"SELECT COUNT(*) as "count: i64" FROM stickers
            WHERE discord_id = ? AND sticker_url = ?"#,
            id,
            sticker_url,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(row.count > 0)
    }
}

fn current_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
