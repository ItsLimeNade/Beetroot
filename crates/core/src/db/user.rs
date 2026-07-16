use crate::crypto;
use crate::error::{CoreError, CoreResult};
use crate::models::user::{DEFAULT_GRAPH_STICKER_COUNT, User, UserDecrypted};

use super::Database;

pub enum TokenUpdate<'a> {
    Keep,
    Clear,
    Set(&'a str),
}

impl User {
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
            mbg_expiry_time: self.mbg_expiry_time.unwrap_or(30),
            last_seen_version: self.last_seen_version,
            bg_image_mode: self.bg_image_mode.unwrap_or(false),
            active_theme: self.active_theme,
            treatment_mode: self
                .treatment_mode
                .unwrap_or_else(|| "contextual".to_string()),
            graph_sticker_count: self
                .graph_sticker_count
                .unwrap_or(DEFAULT_GRAPH_STICKER_COUNT),
            telemetry_accepted: self.telemetry_accepted,
        })
    }
}

impl Database {
    pub async fn get_user(&self, discord_id: u64) -> CoreResult<Option<UserDecrypted>> {
        let id = discord_id as i64;

        let row = sqlx::query_as::<_, User>("SELECT * FROM users WHERE discord_id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        row.map(User::into_decrypted).transpose()
    }

    pub async fn user_exists(&self, discord_id: u64) -> CoreResult<bool> {
        let id = discord_id as i64;

        let exists: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE discord_id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;

        Ok(exists.0 > 0)
    }

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
            "INSERT INTO users (discord_id, nightscout_url, nightscout_token, is_private, graph_sticker_count)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(discord_id) DO UPDATE SET
                 nightscout_url = excluded.nightscout_url,
                 nightscout_token = excluded.nightscout_token,
                 is_private = excluded.is_private",
        )
        .bind(id)
        .bind(url)
        .bind(&encrypted_token)
        .bind(is_private)
        .bind(DEFAULT_GRAPH_STICKER_COUNT)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn set_nightscout_url(&self, discord_id: u64, url: &str) -> CoreResult<()> {
        let id = discord_id as i64;
        sqlx::query("UPDATE users SET nightscout_url = ? WHERE discord_id = ?")
            .bind(url)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_nightscout_token(
        &self,
        discord_id: u64,
        update: TokenUpdate<'_>,
    ) -> CoreResult<()> {
        let id = discord_id as i64;
        match update {
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

    pub async fn set_privacy(&self, discord_id: u64, is_private: bool) -> CoreResult<()> {
        let id = discord_id as i64;
        sqlx::query("UPDATE users SET is_private = ? WHERE discord_id = ?")
            .bind(is_private)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_microbolus_threshold(
        &self,
        discord_id: u64,
        threshold: f64,
    ) -> CoreResult<()> {
        let id = discord_id as i64;
        sqlx::query("UPDATE users SET microbolus_threshold = ? WHERE discord_id = ?")
            .bind(threshold)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_display_microbolus(&self, discord_id: u64, value: bool) -> CoreResult<()> {
        let id = discord_id as i64;
        sqlx::query("UPDATE users SET display_microbolus = ? WHERE discord_id = ?")
            .bind(value)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_force_ephemeral(&self, discord_id: u64, value: bool) -> CoreResult<()> {
        let id = discord_id as i64;
        sqlx::query("UPDATE users SET force_ephemeral = ? WHERE discord_id = ?")
            .bind(value)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_bg_image_mode(&self, discord_id: u64, value: bool) -> CoreResult<()> {
        let id = discord_id as i64;
        sqlx::query("UPDATE users SET bg_image_mode = ? WHERE discord_id = ?")
            .bind(value)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_mbg_expiry_time(&self, discord_id: u64, value: i64) -> CoreResult<()> {
        let id = discord_id as i64;
        sqlx::query("UPDATE users SET mbg_expiry_time = ? WHERE discord_id = ?")
            .bind(value)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Set how treatments are drawn on `/graph` (`"contextual"` or `"timeline"`).
    pub async fn set_treatment_mode(&self, discord_id: u64, mode: &str) -> CoreResult<()> {
        let id = discord_id as i64;
        sqlx::query("UPDATE users SET treatment_mode = ? WHERE discord_id = ?")
            .bind(mode)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Set how many stickers `/graph` scatters on the chart (0 to 30).
    pub async fn set_graph_sticker_count(&self, discord_id: u64, count: i64) -> CoreResult<()> {
        let id = discord_id as i64;
        sqlx::query("UPDATE users SET graph_sticker_count = ? WHERE discord_id = ?")
            .bind(count)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn add_allowed_user(&self, discord_id: u64, target_id: u64) -> CoreResult<bool> {
        self.modify_user_list(discord_id, "allowed_people", target_id, true)
            .await
    }

    pub async fn remove_allowed_user(&self, discord_id: u64, target_id: u64) -> CoreResult<bool> {
        self.modify_user_list(discord_id, "allowed_people", target_id, false)
            .await
    }

    pub async fn add_blocked_user(&self, discord_id: u64, target_id: u64) -> CoreResult<bool> {
        self.modify_user_list(discord_id, "blocked_people", target_id, true)
            .await
    }

    pub async fn remove_blocked_user(&self, discord_id: u64, target_id: u64) -> CoreResult<bool> {
        self.modify_user_list(discord_id, "blocked_people", target_id, false)
            .await
    }

    async fn modify_user_list(
        &self,
        discord_id: u64,
        column: &str,
        target_id: u64,
        add: bool,
    ) -> CoreResult<bool> {
        if column != "allowed_people" && column != "blocked_people" {
            return Err(CoreError::Other(format!("invalid column: {column}")));
        }

        let id = discord_id as i64;

        let mut tx = self.pool.begin().await?;

        let read_sql = format!("SELECT {column} FROM users WHERE discord_id = ?");
        let row: Option<(Option<String>,)> = sqlx::query_as(&read_sql)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

        let raw = match row {
            Some((value,)) => value.unwrap_or_else(|| "[]".to_string()),
            None => return Ok(false),
        };

        let mut list: Vec<u64> = serde_json::from_str(&raw).unwrap_or_default();
        let changed = if add {
            if list.contains(&target_id) {
                false
            } else {
                list.push(target_id);
                true
            }
        } else {
            let before = list.len();
            list.retain(|v| *v != target_id);
            list.len() != before
        };

        if changed {
            let serialized = serde_json::to_string(&list)?;
            let write_sql = format!("UPDATE users SET {column} = ? WHERE discord_id = ?");
            sqlx::query(&write_sql)
                .bind(serialized)
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;

        Ok(changed)
    }

    /// Record the user's telemetry choice, creating the row if it does not
    /// exist yet. `true` opts in, `false` opts out.
    pub async fn set_telemetry_consent(&self, discord_id: u64, accepted: bool) -> CoreResult<()> {
        self.ensure_user_row(discord_id).await?;
        let id = discord_id as i64;
        sqlx::query("UPDATE users SET telemetry_accepted = ? WHERE discord_id = ?")
            .bind(accepted)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Whether the user has a stored row but has never made a telemetry choice
    /// (their `telemetry_accepted` is NULL). These users predate the consent
    /// prompt and are shown a one-time apology + choice.
    pub async fn needs_telemetry_prompt(&self, discord_id: u64) -> CoreResult<bool> {
        let id = discord_id as i64;
        let row: Option<(Option<bool>,)> =
            sqlx::query_as("SELECT telemetry_accepted FROM users WHERE discord_id = ?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(matches!(row, Some((None,))))
    }

    /// The user's telemetry choice: `Some(true/false)` once made, `None` if the
    /// user has never been asked.
    pub async fn get_telemetry_consent(&self, discord_id: u64) -> CoreResult<Option<bool>> {
        let id = discord_id as i64;
        let row: Option<(Option<bool>,)> =
            sqlx::query_as("SELECT telemetry_accepted FROM users WHERE discord_id = ?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.and_then(|(v,)| v))
    }

    pub async fn ensure_user_row(&self, discord_id: u64) -> CoreResult<()> {
        let id = discord_id as i64;
        sqlx::query("INSERT OR IGNORE INTO users (discord_id, graph_sticker_count) VALUES (?, ?)")
            .bind(id)
            .bind(DEFAULT_GRAPH_STICKER_COUNT)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

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

    pub async fn increment_command_count(&self, discord_id: u64) -> CoreResult<u64> {
        let id = discord_id as i64;

        let row: Option<(i64,)> = sqlx::query_as(
            "UPDATE users SET command_count = command_count + 1 WHERE discord_id = ? RETURNING command_count",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(c,)| c as u64).unwrap_or(0))
    }

    /// Erase every trace of a user. The `users` row cascades to stickers and
    /// themes via FK, but `command_logs` and `seen_tips` key on the Discord id
    /// without a foreign key, so they must be cleared explicitly or a "deleted"
    /// user's id lingers in analytics. Done in one transaction so deletion is
    /// all-or-nothing.
    pub async fn delete_user(&self, discord_id: u64) -> CoreResult<()> {
        let id = discord_id as i64;

        let mut tx = self.pool.begin().await?;

        let result = sqlx::query("DELETE FROM users WHERE discord_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        sqlx::query("DELETE FROM command_logs WHERE user_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        sqlx::query("DELETE FROM seen_tips WHERE discord_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        // No user id here: core stays PII-free in logs.
        tracing::debug!(
            rows = result.rows_affected(),
            "deleted user (cascades stickers/themes; purged command_logs + seen_tips)"
        );

        Ok(())
    }
}
