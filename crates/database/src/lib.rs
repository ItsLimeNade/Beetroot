// crates/database/src/lib.rs
mod crypto;
pub mod models;

use crate::models::{
    Analytics, CommandData, ProcessData, Sticker, StickerCategory, UsageData, UserData,
};
use anyhow::Result;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
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

        let user = sqlx::query!(
            r#"SELECT * FROM users WHERE discord_id = ?"#,
            id
        )
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

            // Manual mapping since category is a String in DB but we used an enum logic
            match row.category.as_str() {
                "in_range" => in_range.push(sticker),
                "low" => low.push(sticker),
                "high" => high.push(sticker),
                _ => other.push(sticker),
            }
        }

        // Fix for the type error: use .as_str() to ensure it's a slice
        let nightscout_token = user.nightscout_token
            .as_ref() // Use as_ref() to borrow the String inside Option
            .and_then(|t| crypto::decrypt_token(t.as_str()).ok());
            
        let allowed_people: Vec<u64> = serde_json::from_str(&user.allowed_people.unwrap_or_else(|| "[]".into()))?;
        let blocked_people: Vec<u64> = serde_json::from_str(&user.blocked_people.unwrap_or_else(|| "[]".into()))?;

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

    // ... (Log functions remain the same) ...
    pub async fn log_command_execution(&self, command: &str, user_id: u64, duration_ms: u64) -> Result<()> {
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
            .fetch_one(&self.pool).await?.c;
        let val = now - 86400;
        let daily_active: i64 = sqlx::query!(
            "SELECT COUNT(DISTINCT user_id) as c FROM command_logs WHERE created_at > ?", 
            val,
        ).fetch_one(&self.pool).await?.c;

        let monthly_active: i64 = sqlx::query!(
            "SELECT COUNT(DISTINCT user_id) as c FROM command_logs WHERE created_at > ?", 
            month_ago
        ).fetch_one(&self.pool).await?.c;

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
}

fn current_time() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}