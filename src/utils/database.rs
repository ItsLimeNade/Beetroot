use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use base64::{Engine as _, engine::general_purpose};
use serde_json;
use sqlx::{
    Row, SqlitePool as Pool,
    sqlite::{SqliteConnectOptions, SqlitePool},
};

/// Secure token encryption/decryption module
struct TokenCrypto {
    cipher: Aes256Gcm,
}

impl TokenCrypto {
    /// Create a new TokenCrypto instance with a derived key
    fn new() -> Self {
        let key = Self::derive_key();
        let cipher = Aes256Gcm::new(&key);
        Self { cipher }
    }

    /// Derive a deterministic encryption key from environment
    fn derive_key() -> Key<Aes256Gcm> {
        let salt = dotenvy::var("ENCRYPTION_SALT")
            .unwrap_or_else(|_| "beetroot_default_salt_change_in_production".to_string());

        let key_material = format!("beetroot_token_encryption_v1_{}", salt);
        let hash = blake3::hash(key_material.as_bytes());

        *Key::<Aes256Gcm>::from_slice(hash.as_bytes())
    }

    /// Encrypt a token string
    fn encrypt(&self, plaintext: &str) -> Result<String, Box<dyn std::error::Error>> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = self
            .cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|e| format!("Encryption failed: {}", e))?;

        let mut combined = nonce.to_vec();
        combined.extend_from_slice(&ciphertext);

        Ok(general_purpose::STANDARD.encode(combined))
    }

    /// Decrypt a token string
    fn decrypt(&self, encrypted: &str) -> Result<String, Box<dyn std::error::Error>> {
        let combined = general_purpose::STANDARD.decode(encrypted)?;

        if combined.len() < 12 {
            return Err("Invalid encrypted data".into());
        }

        let (nonce_bytes, ciphertext) = combined.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = self
            .cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| format!("Decryption failed: {}", e))?;

        Ok(String::from_utf8(plaintext)?)
    }
}

/// Global encryption instance - safely managed with OnceLock
static CRYPTO_INSTANCE: std::sync::OnceLock<TokenCrypto> = std::sync::OnceLock::new();

fn get_crypto() -> &'static TokenCrypto {
    CRYPTO_INSTANCE.get_or_init(TokenCrypto::new)
}

#[derive(Clone, Debug)]
pub struct NightscoutInfo {
    pub nightscout_url: Option<String>,
    pub nightscout_token: Option<String>,
    pub allowed_people: Vec<u64>,
    pub is_private: bool,
    pub microbolus_threshold: f32,
    pub display_microbolus: bool,
}

#[derive(Clone, Debug)]
pub struct UserInfo {
    pub nightscout: NightscoutInfo,
    #[allow(dead_code)]
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

        let migration = crate::utils::migration::Migration::new(pool.clone());
        migration.add_microbolus_fields().await?;

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
                nightscout_token TEXT,
                microbolus_threshold REAL DEFAULT 0.5,
                display_microbolus INTEGER DEFAULT 1
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

        let encrypted_token = if let Some(ref token) = nightscout_info.nightscout_token {
            match get_crypto().encrypt(token) {
                Ok(encrypted) => {
                    tracing::debug!("[ENCRYPTION] Token encrypted for user {}", discord_id);
                    Some(encrypted)
                }
                Err(e) => {
                    tracing::error!(
                        "[ENCRYPTION] Failed to encrypt token for user {}: {}",
                        discord_id,
                        e
                    );
                    return Err(sqlx::Error::Protocol("Token encryption failed".to_string()));
                }
            }
        } else {
            None
        };

        sqlx::query(
            "INSERT INTO users (discord_id, nightscout_url, nightscout_token, is_private, allowed_people, microbolus_threshold, display_microbolus) VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(discord_id as i64)
        .bind(&nightscout_info.nightscout_url)
        .bind(&encrypted_token)
        .bind(nightscout_info.is_private as i32)
        .bind(allowed_people_json)
        .bind(nightscout_info.microbolus_threshold)
        .bind(nightscout_info.display_microbolus as i32)
        .execute(&self.pool)
        .await?;

        tracing::info!(
            "[SECURITY] User {} token stored with encryption",
            discord_id
        );
        Ok(())
    }

    pub async fn update_user(
        &self,
        discord_id: u64,
        nightscout_info: NightscoutInfo,
    ) -> Result<(), sqlx::Error> {
        let allowed_people_json =
            serde_json::to_string(&nightscout_info.allowed_people).unwrap_or("[]".to_string());

        let encrypted_token = if let Some(ref token) = nightscout_info.nightscout_token {
            match get_crypto().encrypt(token) {
                Ok(encrypted) => {
                    tracing::debug!("[ENCRYPTION] Token encrypted for user {}", discord_id);
                    Some(encrypted)
                }
                Err(e) => {
                    tracing::error!(
                        "[ENCRYPTION] Failed to encrypt token for user {}: {}",
                        discord_id,
                        e
                    );
                    return Err(sqlx::Error::Protocol("Token encryption failed".to_string()));
                }
            }
        } else {
            None
        };

        sqlx::query(
            "UPDATE users SET nightscout_url = ?, nightscout_token = ?, is_private = ?, allowed_people = ?, microbolus_threshold = ?, display_microbolus = ? WHERE discord_id = ?"
        )
        .bind(&nightscout_info.nightscout_url)
        .bind(&encrypted_token)
        .bind(nightscout_info.is_private as i32)
        .bind(allowed_people_json)
        .bind(nightscout_info.microbolus_threshold)
        .bind(nightscout_info.display_microbolus as i32)
        .bind(discord_id as i64)
        .execute(&self.pool)
        .await?;

        tracing::info!(
            "[SECURITY] User {} token updated with encryption",
            discord_id
        );
        Ok(())
    }
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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

    #[allow(dead_code)]
    pub async fn delete_sticker(&self, sticker_id: i32) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM stickers WHERE id = ?")
            .bind(sticker_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn get_nightscout_info(&self, user_id: u64) -> Result<NightscoutInfo, sqlx::Error> {
        let row = sqlx::query(
            "SELECT nightscout_url, nightscout_token, is_private, allowed_people, microbolus_threshold, display_microbolus FROM users WHERE discord_id = ?"
        )
        .bind(user_id as i64)
        .fetch_one(&self.pool).await?;

        let nightscout_url: Option<String> = row.get("nightscout_url");
        let encrypted_token: Option<String> = row.get("nightscout_token");
        let is_private: bool = row.get::<i32, _>("is_private") != 0;
        let allowed_people: Vec<u64> =
            serde_json::from_str(&row.get::<String, _>("allowed_people")).unwrap_or_default();
        let microbolus_threshold: f32 = row.get::<Option<f32>, _>("microbolus_threshold").unwrap_or(0.5);
        let display_microbolus: bool = row.get::<Option<i32>, _>("display_microbolus").unwrap_or(1) != 0;

        let nightscout_token = if let Some(encrypted) = encrypted_token {
            match get_crypto().decrypt(&encrypted) {
                Ok(decrypted) => {
                    tracing::debug!("[ENCRYPTION] Token decrypted for user {}", user_id);
                    Some(decrypted)
                }
                Err(e) => {
                    tracing::error!(
                        "[ENCRYPTION] Failed to decrypt token for user {}: {}",
                        user_id,
                        e
                    );
                    tracing::warn!(
                        "[ENCRYPTION] User {} may need to re-enter their token",
                        user_id
                    );
                    None
                }
            }
        } else {
            None
        };

        let info = NightscoutInfo {
            nightscout_url,
            nightscout_token,
            is_private,
            allowed_people,
            microbolus_threshold,
            display_microbolus,
        };

        Ok(info)
    }

    /// Migrate existing unencrypted tokens to encrypted format
    /// This should be run once after deploying the encryption feature
    #[allow(dead_code)]
    pub async fn migrate_tokens_to_encrypted(&self) -> Result<u32, sqlx::Error> {
        tracing::info!("[MIGRATION] Starting token encryption migration");

        let rows = sqlx::query(
            "SELECT discord_id, nightscout_token FROM users WHERE nightscout_token IS NOT NULL",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut migrated_count = 0;

        for row in rows {
            let discord_id: i64 = row.get("discord_id");
            let current_token: String = row.get("nightscout_token");

            if current_token.len() > 100 && general_purpose::STANDARD.decode(&current_token).is_ok()
            {
                tracing::debug!(
                    "[MIGRATION] Token for user {} appears already encrypted, skipping",
                    discord_id
                );
                continue;
            }

            match get_crypto().encrypt(&current_token) {
                Ok(encrypted_token) => {
                    sqlx::query("UPDATE users SET nightscout_token = ? WHERE discord_id = ?")
                        .bind(&encrypted_token)
                        .bind(discord_id)
                        .execute(&self.pool)
                        .await?;

                    migrated_count += 1;
                    tracing::info!("[MIGRATION] Encrypted token for user {}", discord_id);
                }
                Err(e) => {
                    tracing::error!(
                        "[MIGRATION] Failed to encrypt token for user {}: {}",
                        discord_id,
                        e
                    );
                }
            }
        }

        tracing::info!(
            "[MIGRATION] Completed token encryption migration: {} tokens encrypted",
            migrated_count
        );
        Ok(migrated_count)
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

    pub async fn update_microbolus_settings(
        &self,
        discord_id: u64,
        threshold: f32,
        display: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE users SET microbolus_threshold = ?, display_microbolus = ? WHERE discord_id = ?"
        )
        .bind(threshold)
        .bind(display as i32)
        .bind(discord_id as i64)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
