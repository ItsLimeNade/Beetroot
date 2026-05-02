use crate::error::CoreResult;
use crate::models::session::DashboardSession;

use super::Database;

impl Database {
    /// Insert a new session.
    pub async fn create_session(&self, session: &DashboardSession) -> CoreResult<()> {
        sqlx::query(
            "INSERT INTO dashboard_sessions
                (id, discord_id, discord_username, discord_avatar, created_at, last_active_at, expires_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&session.id)
        .bind(session.discord_id)
        .bind(&session.discord_username)
        .bind(&session.discord_avatar)
        .bind(session.created_at)
        .bind(session.last_active_at)
        .bind(session.expires_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Look up a session by its token, only if not expired.
    pub async fn get_session(&self, session_id: &str) -> CoreResult<Option<DashboardSession>> {
        let now = now_secs();

        let session = sqlx::query_as::<_, DashboardSession>(
            "SELECT * FROM dashboard_sessions WHERE id = ? AND expires_at > ?",
        )
        .bind(session_id)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;

        Ok(session)
    }

    /// Bump `last_active_at` so we know when the user was last seen.
    pub async fn touch_session(&self, session_id: &str) -> CoreResult<()> {
        let now = now_secs();

        sqlx::query("UPDATE dashboard_sessions SET last_active_at = ? WHERE id = ?")
            .bind(now)
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Delete a single session (logout).
    pub async fn delete_session(&self, session_id: &str) -> CoreResult<()> {
        sqlx::query("DELETE FROM dashboard_sessions WHERE id = ?")
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Delete all sessions for a user (logout everywhere).
    pub async fn delete_user_sessions(&self, discord_id: u64) -> CoreResult<()> {
        let id = discord_id as i64;

        sqlx::query("DELETE FROM dashboard_sessions WHERE discord_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Remove all expired sessions. Call periodically to keep the table clean.
    pub async fn purge_expired_sessions(&self) -> CoreResult<u64> {
        let now = now_secs();

        let result = sqlx::query("DELETE FROM dashboard_sessions WHERE expires_at <= ?")
            .bind(now)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected())
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_secs() as i64
}
