use serde_json;
use sqlx::{
    Row, SqlitePool as Pool,
    sqlite::{SqliteConnectOptions, SqlitePool},
};

#[derive(Clone, Debug)]
pub struct NightscoutInfo {
    pub nightscout_url: Option<String>,
    pub nightscout_token: Option<String>,
    pub allowed_people: Vec<u64>,
    pub is_private: bool,
}

#[derive(Clone, Debug)]
pub struct UserInfo {
    pub nightscout: NightscoutInfo,
    pub stickers: Vec<String>,
}

pub struct Database {
    pool: Pool,
}

impl Database {
    pub async fn new() -> Result<Self, sqlx::Error> {
        let opts = SqliteConnectOptions::new()
            .filename("db.sqlite")
            .create_if_missing(true);

        let pool = SqlitePool::connect_with(opts).await?;

        Self::setup_tables(&pool).await?;

        Ok(Database { pool })
    }

    async fn setup_tables(pool: &Pool) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS users (
                discord_id INTEGER PRIMARY KEY,
                allowed_people TEXT DEFAULT '[]',
                is_private INTEGER NOT NULL DEFAULT 1,
                nightscout_url TEXT,
                nightscout_token TEXT
            )
            "#,
        )
        .execute(pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS stickers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_name TEXT NOT NULL,
                discord_id INTEGER NOT NULL,
                FOREIGN KEY (discord_id) REFERENCES users(discord_id)
            )
            "#,
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn get_user_info(&self, user_id: u64) -> Result<UserInfo, sqlx::Error> {
        let nightscout = self.get_nightscout_info(user_id).await?;
        let stickers = self.get_user_stickers(user_id).await?;

        Ok(UserInfo {
            nightscout,
            stickers,
        })
    }

    pub async fn user_exists(&self, discord_id: u64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("SELECT 1 FROM users WHERE discord_id = ? LIMIT 1")
            .bind(discord_id as i64)
            .fetch_optional(&self.pool)
            .await?;

        Ok(result.is_some())
    }

    pub async fn insert_user(
        &self,
        discord_id: u64,
        nightscout_info: NightscoutInfo,
    ) -> Result<(), sqlx::Error> {
        let allowed_people_json =
            serde_json::to_string(&nightscout_info.allowed_people).unwrap_or("[]".to_string());

        sqlx::query(
            "INSERT INTO users (discord_id, nightscout_url, nightscout_token, is_private, allowed_people) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(discord_id as i64)
        .bind(&nightscout_info.nightscout_url)
        .bind(&nightscout_info.nightscout_token)
        .bind(nightscout_info.is_private as i32)
        .bind(allowed_people_json)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_user(
        &self,
        discord_id: u64,
        nightscout_info: NightscoutInfo,
    ) -> Result<(), sqlx::Error> {
        let allowed_people_json =
            serde_json::to_string(&nightscout_info.allowed_people).unwrap_or("[]".to_string());

        sqlx::query(
            "UPDATE users SET nightscout_url = ?, nightscout_token = ?, is_private = ?, allowed_people = ? WHERE discord_id = ?"
        )
        .bind(&nightscout_info.nightscout_url)
        .bind(&nightscout_info.nightscout_token)
        .bind(nightscout_info.is_private as i32)
        .bind(allowed_people_json)
        .bind(discord_id as i64)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_user(&self, discord_id: u64) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM stickers WHERE discord_id = ?")
            .bind(discord_id as i64)
            .execute(&self.pool)
            .await?;

        sqlx::query("DELETE FROM users WHERE discord_id = ?")
            .bind(discord_id as i64)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn insert_sticker(
        &self,
        discord_id: u64,
        file_name: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO stickers (file_name, discord_id) VALUES (?, ?)")
            .bind(file_name)
            .bind(discord_id as i64)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn delete_sticker(&self, sticker_id: i32) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM stickers WHERE id = ?")
            .bind(sticker_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn get_nightscout_info(&self, user_id: u64) -> Result<NightscoutInfo, sqlx::Error> {
        let row = sqlx::query(
            "SELECT nightscout_url, nightscout_token, is_private, allowed_people FROM users WHERE discord_id = ?"
        )
        .bind(user_id as i64)
        .fetch_one(&self.pool).await?;

        let nightscout_url: Option<String> = row.get("nightscout_url");
        let nightscout_token: Option<String> = row.get("nightscout_token");
        let is_private: bool = row.get::<i32, _>("is_private") != 0;
        let allowed_people: Vec<u64> =
            serde_json::from_str(&row.get::<String, _>("allowed_people")).unwrap_or_default();

        let info = NightscoutInfo {
            nightscout_url,
            nightscout_token,
            is_private,
            allowed_people,
        };

        Ok(info)
    }

    async fn get_user_stickers(&self, user_id: u64) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query("SELECT file_name FROM stickers WHERE discord_id = ?")
            .bind(user_id as i64)
            .fetch_all(&self.pool)
            .await?;

        let mut sticker_paths: Vec<String> = rows
            .iter()
            .map(|f| f.get::<String, _>("file_name"))
            .collect();

        if sticker_paths.is_empty() {
            sticker_paths = vec![
                "images/stickers/thing.webp".to_string(),
                "images/stickers/thing2.webp".to_string(),
                "images/stickers/thing3.webp".to_string(),
            ];
        }

        Ok(sticker_paths)
    }
}
