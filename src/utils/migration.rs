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

    pub async fn add_sticker_display_name_field(&self) -> Result<(), sqlx::Error> {
        tracing::info!("[MIGRATION] Adding display_name field to stickers table");

        let check_display_name_query = sqlx::query(
            "SELECT COUNT(*) as count FROM pragma_table_info('stickers') WHERE name = 'display_name'",
        );

        let display_name_exists = check_display_name_query
            .fetch_one(&self.pool)
            .await?
            .get::<i32, _>("count")
            > 0;

        if !display_name_exists {
            sqlx::query("ALTER TABLE stickers ADD COLUMN display_name TEXT DEFAULT ''")
                .execute(&self.pool)
                .await?;
            tracing::info!("[MIGRATION] Added display_name column");

            // Update existing stickers with extracted display names
            // For old Discord stickers, we'll just use a generic name since we can't extract the original name
            sqlx::query(
                "UPDATE stickers SET display_name =
                 CASE
                   WHEN file_name LIKE 'https://media.discordapp.net/stickers/%' THEN 'Discord Sticker'
                   WHEN file_name LIKE 'http%' THEN 'Custom Sticker'
                   ELSE COALESCE(
                     REPLACE(
                       REPLACE(
                         SUBSTR(file_name, INSTR(file_name, '/') + 1),
                         '.webp', ''
                       ),
                       '.png', ''
                     ),
                     'Unknown'
                   )
                 END
                 WHERE display_name = '' OR display_name IS NULL"
            )
            .execute(&self.pool)
            .await?;
            tracing::info!("[MIGRATION] Updated existing stickers with display names");
        }

        tracing::info!("[MIGRATION] Sticker display name field migration completed");
        Ok(())
    }

    pub async fn add_last_seen_version_field(&self) -> Result<(), sqlx::Error> {
        tracing::info!("[MIGRATION] Adding last_seen_version field to users table");

        let check_version_query = sqlx::query(
            "SELECT COUNT(*) as count FROM pragma_table_info('users') WHERE name = 'last_seen_version'",
        );

        let version_exists = check_version_query
            .fetch_one(&self.pool)
            .await?
            .get::<i32, _>("count")
            > 0;

        if !version_exists {
            sqlx::query("ALTER TABLE users ADD COLUMN last_seen_version TEXT DEFAULT '0.1.0'")
                .execute(&self.pool)
                .await?;
            tracing::info!("[MIGRATION] Added last_seen_version column");
        }

        tracing::info!("[MIGRATION] Last seen version field migration completed");
        Ok(())
    }
}
