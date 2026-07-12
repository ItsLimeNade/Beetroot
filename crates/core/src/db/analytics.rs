use crate::error::CoreResult;
use crate::models::analytics::{CommandStats, UsageStats, UserDataSummary};

use super::Database;

impl Database {
    /// Record that a slash command was executed.
    ///
    /// This is gated on telemetry consent: the row is only written when the
    /// user has explicitly opted in (`telemetry_accepted = 1`). Undecided or
    /// opted-out users are never recorded. The check is part of the same
    /// statement so there is no separate lookup.
    pub async fn log_command_execution(
        &self,
        command: &str,
        user_id: u64,
        duration_ms: u64,
    ) -> CoreResult<()> {
        let now = now_secs();
        let uid = user_id as i64;
        let dur = duration_ms as i64;

        let result = sqlx::query(
            "INSERT INTO command_logs (command_name, user_id, execution_time_ms, created_at)
             SELECT ?, ?, ?, ?
             WHERE EXISTS (
                 SELECT 1 FROM users WHERE discord_id = ? AND telemetry_accepted = 1
             )",
        )
        .bind(command)
        .bind(uid)
        .bind(dur)
        .bind(now)
        .bind(uid)
        .execute(&self.pool)
        .await?;

        tracing::trace!(
            command,
            duration_ms = dur,
            recorded = result.rows_affected() > 0,
            "command execution (telemetry gated)"
        );
        Ok(())
    }

    /// Everything the telemetry table holds about one user, for a data request.
    pub async fn get_user_data_summary(&self, user_id: u64) -> CoreResult<UserDataSummary> {
        let uid = user_id as i64;

        let (total, first_at, last_at): (i64, Option<i64>, Option<i64>) = sqlx::query_as(
            "SELECT COUNT(*), MIN(created_at), MAX(created_at)
             FROM command_logs WHERE user_id = ?",
        )
        .bind(uid)
        .fetch_one(&self.pool)
        .await?;

        let per_command: Vec<(String, i64)> = sqlx::query_as(
            "SELECT command_name, COUNT(*)
             FROM command_logs WHERE user_id = ?
             GROUP BY command_name
             ORDER BY COUNT(*) DESC",
        )
        .bind(uid)
        .fetch_all(&self.pool)
        .await?;

        Ok(UserDataSummary {
            command_log_count: total,
            first_at,
            last_at,
            per_command,
        })
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
