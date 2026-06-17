use crate::error::CoreResult;
use crate::models::theme::ThemeRow;

use super::Database;

impl Database {
    /// Insert a new custom theme. Returns `Ok(true)` when the row was created,
    /// `Ok(false)` when the user already has a theme with that name.
    pub async fn insert_theme(&self, discord_id: u64, name: &str, data: &str) -> CoreResult<bool> {
        let id = discord_id as i64;

        let result =
            sqlx::query("INSERT OR IGNORE INTO themes (discord_id, name, data) VALUES (?, ?, ?)")
                .bind(id)
                .bind(name)
                .bind(data)
                .execute(&self.pool)
                .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Overwrite the color data of an existing theme. Returns `Ok(true)` when a
    /// matching theme was updated.
    pub async fn update_theme_data(
        &self,
        discord_id: u64,
        name: &str,
        data: &str,
    ) -> CoreResult<bool> {
        let id = discord_id as i64;

        let result = sqlx::query("UPDATE themes SET data = ? WHERE discord_id = ? AND name = ?")
            .bind(data)
            .bind(id)
            .bind(name)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Fetch a single custom theme by its (user, name) pair.
    pub async fn get_theme_by_name(
        &self,
        discord_id: u64,
        name: &str,
    ) -> CoreResult<Option<ThemeRow>> {
        let id = discord_id as i64;

        let row = sqlx::query_as::<_, ThemeRow>(
            "SELECT id, discord_id, name, data FROM themes WHERE discord_id = ? AND name = ?",
        )
        .bind(id)
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    /// Fetch all of a user's custom themes, ordered by creation time.
    pub async fn get_user_themes(&self, discord_id: u64) -> CoreResult<Vec<ThemeRow>> {
        let id = discord_id as i64;

        let rows = sqlx::query_as::<_, ThemeRow>(
            "SELECT id, discord_id, name, data FROM themes
             WHERE discord_id = ? ORDER BY created_at ASC, id ASC",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    /// Count how many custom themes a user has (used to enforce a per-user cap).
    pub async fn count_user_themes(&self, discord_id: u64) -> CoreResult<i64> {
        let id = discord_id as i64;

        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM themes WHERE discord_id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;

        Ok(count)
    }

    /// Delete a custom theme by name. Returns `Ok(true)` when a row was removed.
    pub async fn delete_theme(&self, discord_id: u64, name: &str) -> CoreResult<bool> {
        let id = discord_id as i64;

        let result = sqlx::query("DELETE FROM themes WHERE discord_id = ? AND name = ?")
            .bind(id)
            .bind(name)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Set (or clear, with `None`) the user's active theme selector.
    pub async fn set_active_theme(&self, discord_id: u64, value: Option<&str>) -> CoreResult<()> {
        let id = discord_id as i64;
        sqlx::query("UPDATE users SET active_theme = ? WHERE discord_id = ?")
            .bind(value)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::Database;
    use sqlx::sqlite::SqlitePoolOptions;

    /// Single-connection in-memory pool with all migrations applied.
    async fn migrated_db() -> Database {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("create in-memory sqlite");
        sqlx::migrate!().run(&pool).await.expect("run migrations");
        Database::new(pool)
    }

    #[tokio::test]
    async fn theme_crud_and_active_selector() {
        let db = migrated_db().await;
        let uid = 42u64;
        db.ensure_user_row(uid).await.expect("ensure user");

        // create
        assert!(db.insert_theme(uid, "mine", "{\"a\":1}").await.unwrap());
        // duplicate name rejected
        assert!(!db.insert_theme(uid, "mine", "{}").await.unwrap());
        assert_eq!(db.count_user_themes(uid).await.unwrap(), 1);

        // read
        let row = db.get_theme_by_name(uid, "mine").await.unwrap().unwrap();
        assert_eq!(row.data, "{\"a\":1}");

        // update
        assert!(
            db.update_theme_data(uid, "mine", "{\"b\":2}")
                .await
                .unwrap()
        );
        let row = db.get_theme_by_name(uid, "mine").await.unwrap().unwrap();
        assert_eq!(row.data, "{\"b\":2}");

        // active selector round-trips through the new users column
        db.set_active_theme(uid, Some("custom:mine")).await.unwrap();
        let user = db.get_user(uid).await.unwrap().unwrap();
        assert_eq!(user.active_theme.as_deref(), Some("custom:mine"));

        // delete
        assert!(db.delete_theme(uid, "mine").await.unwrap());
        assert!(!db.delete_theme(uid, "mine").await.unwrap());
        assert_eq!(db.count_user_themes(uid).await.unwrap(), 0);
    }
}
