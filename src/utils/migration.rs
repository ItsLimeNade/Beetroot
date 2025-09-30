use sqlx::{Row, SqlitePool};

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
            "SELECT COUNT(*) as count FROM pragma_table_info('users') WHERE name = 'microbolus_threshold'",
        );

        let threshold_exists = check_threshold_query
            .fetch_one(&self.pool)
            .await?
            .get::<i32, _>("count")
            > 0;

        if !threshold_exists {
            sqlx::query("ALTER TABLE users ADD COLUMN microbolus_threshold REAL DEFAULT 0.5")
                .execute(&self.pool)
                .await?;
            tracing::info!("[MIGRATION] Added microbolus_threshold column");
        }

        let check_display_query = sqlx::query(
            "SELECT COUNT(*) as count FROM pragma_table_info('users') WHERE name = 'display_microbolus'",
        );

        let display_exists = check_display_query
            .fetch_one(&self.pool)
            .await?
            .get::<i32, _>("count")
            > 0;

        if !display_exists {
            sqlx::query("ALTER TABLE users ADD COLUMN display_microbolus INTEGER DEFAULT 1")
                .execute(&self.pool)
                .await?;
            tracing::info!("[MIGRATION] Added display_microbolus column");
        }

        tracing::info!("[MIGRATION] Microbolus fields migration completed");
        Ok(())
    }

    pub async fn add_sticker_position_fields(&self) -> Result<(), sqlx::Error> {
        tracing::info!("[MIGRATION] Adding position and rotation fields to stickers table");

        let check_x_position_query = sqlx::query(
            "SELECT COUNT(*) as count FROM pragma_table_info('stickers') WHERE name = 'x_position'",
        );

        let x_position_exists = check_x_position_query
            .fetch_one(&self.pool)
            .await?
            .get::<i32, _>("count")
            > 0;

        if !x_position_exists {
            sqlx::query("ALTER TABLE stickers ADD COLUMN x_position REAL DEFAULT 0.5")
                .execute(&self.pool)
                .await?;
            tracing::info!("[MIGRATION] Added x_position column");
        }

        let check_y_position_query = sqlx::query(
            "SELECT COUNT(*) as count FROM pragma_table_info('stickers') WHERE name = 'y_position'",
        );

        let y_position_exists = check_y_position_query
            .fetch_one(&self.pool)
            .await?
            .get::<i32, _>("count")
            > 0;

        if !y_position_exists {
            sqlx::query("ALTER TABLE stickers ADD COLUMN y_position REAL DEFAULT 0.5")
                .execute(&self.pool)
                .await?;
            tracing::info!("[MIGRATION] Added y_position column");
        }

        let check_rotation_query = sqlx::query(
            "SELECT COUNT(*) as count FROM pragma_table_info('stickers') WHERE name = 'rotation'",
        );

        let rotation_exists = check_rotation_query
            .fetch_one(&self.pool)
            .await?
            .get::<i32, _>("count")
            > 0;

        if !rotation_exists {
            sqlx::query("ALTER TABLE stickers ADD COLUMN rotation REAL DEFAULT 0.0")
                .execute(&self.pool)
                .await?;
            tracing::info!("[MIGRATION] Added rotation column");
        }

        tracing::info!("[MIGRATION] Sticker position fields migration completed");
        Ok(())
    }
}
