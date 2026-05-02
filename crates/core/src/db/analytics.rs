use crate::error::CoreResult;
use crate::models::analytics::{CommandStats, UsageStats};

use super::Database;

impl Database {
    /// Record that a slash command was executed.
    pub async fn log_command_execution(
        &self,
        command: &str,
        user_id: u64,
        duration_ms: u64,
    ) -> CoreResult<()> {
        let now = now_secs();
        let uid = user_id as i64;
        let dur = duration_ms as i64;

        sqlx::query(
            "INSERT INTO command_logs (command_name, user_id, execution_time_ms, created_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(command)
        .bind(uid)
        .bind(dur)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Aggregate per-command statistics (total, weekly, monthly, avg time).
    pub async fn get_command_stats(&self) -> CoreResult<Vec<CommandStats>> {
        let now = now_secs();
        let week_ago = now - 604_800;
        let month_ago = now - 2_592_000;

        // SQLx doesn't map these computed columns to a FromRow struct easily,
        // so we use a tuple and build the struct ourselves.
        let rows: Vec<(String, i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT
                 command_name,
                 COUNT(*),
                 SUM(CASE WHEN created_at > ? THEN 1 ELSE 0 END),
                 SUM(CASE WHEN created_at > ? THEN 1 ELSE 0 END),
                 CAST(AVG(execution_time_ms) AS INTEGER)
             FROM command_logs
             GROUP BY command_name",
        )
        .bind(week_ago)
        .bind(month_ago)
        .fetch_all(&self.pool)
        .await?;

        let stats = rows
            .into_iter()
            .map(|(name, total, weekly, monthly, avg)| CommandStats {
                name,
                total_use: total,
                weekly_use: weekly,
                monthly_use: monthly,
                average_execution_time: avg,
            })
            .collect();

        Ok(stats)
    }

    /// Compute user activity counters (total, daily active, monthly active).
    pub async fn get_usage_stats(&self) -> CoreResult<UsageStats> {
        let now = now_secs();
        let day_ago = now - 86_400;
        let month_ago = now - 2_592_000;

        let (total_users,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await?;

        let (daily_active,): (i64,) =
            sqlx::query_as("SELECT COUNT(DISTINCT user_id) FROM command_logs WHERE created_at > ?")
                .bind(day_ago)
                .fetch_one(&self.pool)
                .await?;

        let (monthly_active,): (i64,) =
            sqlx::query_as("SELECT COUNT(DISTINCT user_id) FROM command_logs WHERE created_at > ?")
                .bind(month_ago)
                .fetch_one(&self.pool)
                .await?;

        Ok(UsageStats {
            total_users: total_users as u64,
            daily_active_users: daily_active as u64,
            monthly_active_users: monthly_active as u64,
        })
    }
}

/// Current Unix timestamp in seconds.
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_secs() as i64
}
