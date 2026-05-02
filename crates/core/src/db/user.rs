use crate::crypto;
use crate::error::CoreResult;
use crate::models::user::{User, UserDecrypted};

use super::Database;

/// What to do with the Nightscout token during a settings update.
///
/// The settings form lets the user keep their existing token without
/// re-entering it, replace it, or remove it entirely. Plain `Option<&str>`
/// can't express "leave alone" vs "set to NULL", so we use this enum.
pub enum TokenUpdate<'a> {
    Keep,
    Clear,
    /// Encrypt this value and store it.
    Set(&'a str),
}

impl User {
    /// Decrypt and resolve defaults, producing the in-memory view.
    ///
    /// This is where the `Option<>` fields from the raw SQL row get their
    /// default values applied, matching the `DEFAULT` clauses in the schema.
    fn into_decrypted(self) -> CoreResult<UserDecrypted> {
        let nightscout_token = self
            .nightscout_token
            .as_deref()
            .map(crypto::decrypt_token)
            .transpose()?;

        let allowed_people: Vec<u64> =
            serde_json::from_str(self.allowed_people.as_deref().unwrap_or("[]"))?;

        let blocked_people: Vec<u64> =
            serde_json::from_str(self.blocked_people.as_deref().unwrap_or("[]"))?;

        Ok(UserDecrypted {
            discord_id: self.discord_id as u64,
            nightscout_url: self.nightscout_url,
            nightscout_token,
            allowed_people,
            blocked_people,
            is_private: self.is_private.unwrap_or(true),
            microbolus_threshold: self.microbolus_threshold.unwrap_or(0.5),
            display_microbolus: self.display_microbolus.unwrap_or(true),
            force_ephemeral: self.force_ephemeral.unwrap_or(false),
            mbg_expiry_time: self.mbg_expiry_time.unwrap_or(900),
            last_seen_version: self.last_seen_version,
        })
    }
}

// Queries

impl Database {
    /// Fetch a single user by Discord ID, returning the decrypted view.
    ///
    /// Returns `Ok(None)` if the user doesn't exist.
    pub async fn get_user(&self, discord_id: u64) -> CoreResult<Option<UserDecrypted>> {
        let id = discord_id as i64;

        let row = sqlx::query_as::<_, User>("SELECT * FROM users WHERE discord_id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        row.map(User::into_decrypted).transpose()
    }

    /// Check whether a user row exists.
    pub async fn user_exists(&self, discord_id: u64) -> CoreResult<bool> {
        let id = discord_id as i64;

        let exists: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE discord_id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;

        Ok(exists.0 > 0)
    }

    /// Insert or update a user's Nightscout configuration.
    ///
    /// This is the main "registration" path: if the user doesn't exist yet,
    /// a new row is created with the given values. If they already exist,
    /// only the Nightscout fields and `is_private` are overwritten.
    pub async fn update_user_nightscout(
        &self,
        discord_id: u64,
        url: &str,
        token: Option<&str>,
        is_private: bool,
    ) -> CoreResult<()> {
        let id = discord_id as i64;

        let encrypted_token = match token {
            Some(t) if !t.trim().is_empty() => Some(crypto::encrypt_token(t)?),
            _ => None,
        };

        sqlx::query(
            "INSERT INTO users (discord_id, nightscout_url, nightscout_token, is_private)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(discord_id) DO UPDATE SET
                 nightscout_url = excluded.nightscout_url,
                 nightscout_token = excluded.nightscout_token,
                 is_private = excluded.is_private",
        )
        .bind(id)
        .bind(url)
        .bind(&encrypted_token)
        .bind(is_private)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Persist all dashboard-managed user settings in one go.
    ///
    /// This is intended for the settings page: the user has filled in the
    /// whole form and we overwrite everything they could change. The token
    /// is handled separately via [`TokenUpdate`] so an unchanged form field
    /// doesn't accidentally clear the stored token.
    pub async fn update_user_settings(
        &self,
        discord_id: u64,
        nightscout_url: &str,
        token_update: TokenUpdate<'_>,
        is_private: bool,
        microbolus_threshold: f64,
        display_microbolus: bool,
        force_ephemeral: bool,
    ) -> CoreResult<()> {
        let id = discord_id as i64;

        sqlx::query(
            "UPDATE users SET
                nightscout_url = ?,
                is_private = ?,
                microbolus_threshold = ?,
                display_microbolus = ?,
                force_ephemeral = ?
             WHERE discord_id = ?",
        )
        .bind(nightscout_url)
        .bind(is_private)
        .bind(microbolus_threshold)
        .bind(display_microbolus)
        .bind(force_ephemeral)
        .bind(id)
        .execute(&self.pool)
        .await?;

        match token_update {
            TokenUpdate::Keep => {}
            TokenUpdate::Clear => {
                sqlx::query("UPDATE users SET nightscout_token = NULL WHERE discord_id = ?")
                    .bind(id)
                    .execute(&self.pool)
                    .await?;
            }
            TokenUpdate::Set(t) => {
                let encrypted = crypto::encrypt_token(t)?;
                sqlx::query("UPDATE users SET nightscout_token = ? WHERE discord_id = ?")
                    .bind(&encrypted)
                    .bind(id)
                    .execute(&self.pool)
                    .await?;
            }
        }

        Ok(())
    }

    /// Mark the current changelog version as seen by this user.
    ///
    /// The dashboard reads this to decide whether to show the changelog
    /// modal on next load.
    pub async fn update_user_last_seen_version(
        &self,
        discord_id: u64,
        version: &str,
    ) -> CoreResult<()> {
        let id = discord_id as i64;

        sqlx::query("UPDATE users SET last_seen_version = ? WHERE discord_id = ?")
            .bind(version)
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Delete a user and (via CASCADE) all their stickers.
    pub async fn delete_user(&self, discord_id: u64) -> CoreResult<()> {
        let id = discord_id as i64;

        sqlx::query("DELETE FROM users WHERE discord_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}
