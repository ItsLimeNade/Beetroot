use crate::error::CoreResult;

use super::Database;

impl Database {
    pub async fn get_seen_tip_ids(&self, discord_id: u64) -> CoreResult<Vec<String>> {
        let id = discord_id as i64;
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT tip_id FROM seen_tips WHERE discord_id = ?")
                .bind(id)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(|(t,)| t).collect())
    }

    pub async fn mark_tip_seen(&self, discord_id: u64, tip_id: &str) -> CoreResult<()> {
        let id = discord_id as i64;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        sqlx::query(
            "INSERT OR IGNORE INTO seen_tips (discord_id, tip_id, seen_at) VALUES (?, ?, ?)",
        )
        .bind(id)
        .bind(tip_id)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn clear_seen_tips(&self, discord_id: u64) -> CoreResult<()> {
        let id = discord_id as i64;
        sqlx::query("DELETE FROM seen_tips WHERE discord_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
