use sqlx::{SqlitePool, Row};

pub struct Migration {
    pool: SqlitePool,
}

impl Migration {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn add_microbolus_fields(&self) -> Result<(), sqlx::Error> {
        tracing::info!("[MIGRATION] Adding microbolus fields to users table");

        let check_threshold_query = sqlx::query(
            "SELECT COUNT(*) as count FROM pragma_table_info('users') WHERE name = 'microbolus_threshold'"
        );

        let threshold_exists = check_threshold_query
            .fetch_one(&self.pool)
            .await?
            .get::<i32, _>("count") > 0;

        if !threshold_exists {
            sqlx::query("ALTER TABLE users ADD COLUMN microbolus_threshold REAL DEFAULT 0.5")
                .execute(&self.pool)
                .await?;
            tracing::info!("[MIGRATION] Added microbolus_threshold column");
        }

        let check_display_query = sqlx::query(
            "SELECT COUNT(*) as count FROM pragma_table_info('users') WHERE name = 'display_microbolus'"
        );

        let display_exists = check_display_query
            .fetch_one(&self.pool)
            .await?
            .get::<i32, _>("count") > 0;

        if !display_exists {
            sqlx::query("ALTER TABLE users ADD COLUMN display_microbolus INTEGER DEFAULT 1")
                .execute(&self.pool)
                .await?;
            tracing::info!("[MIGRATION] Added display_microbolus column");
        }

        tracing::info!("[MIGRATION] Microbolus fields migration completed");
        Ok(())
    }
}