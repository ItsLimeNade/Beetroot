use crate::error::CoreResult;
use crate::models::sticker::{Sticker, StickerCategory};

use super::Database;

impl Database {
    /// Insert a new sticker for a user.
    pub async fn insert_sticker(
        &self,
        discord_id: u64,
        sticker_url: &str,
        display_name: &str,
        category: StickerCategory,
    ) -> CoreResult<()> {
        let id = discord_id as i64;

        sqlx::query(
            "INSERT INTO stickers (discord_id, sticker_url, display_name, category)
             VALUES (?, ?, ?, ?)",
        )
        .bind(id)
        .bind(sticker_url)
        .bind(display_name)
        .bind(category)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Delete a sticker by its row ID.
    pub async fn delete_sticker(&self, sticker_id: i64) -> CoreResult<()> {
        sqlx::query("DELETE FROM stickers WHERE id = ?")
            .bind(sticker_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Delete a sticker but only if it belongs to the given user.
    ///
    /// Returns `Ok(true)` when a row was deleted, `Ok(false)` when the
    /// sticker doesn't exist or belongs to someone else. The dashboard
    /// uses this to ensure a user can never remove another user's sticker
    /// by guessing an ID.
    pub async fn delete_user_sticker(&self, discord_id: u64, sticker_id: i64) -> CoreResult<bool> {
        let id = discord_id as i64;

        let result = sqlx::query("DELETE FROM stickers WHERE id = ? AND discord_id = ?")
            .bind(sticker_id)
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete all stickers for a user.
    pub async fn clear_user_stickers(&self, discord_id: u64) -> CoreResult<()> {
        let id = discord_id as i64;

        sqlx::query("DELETE FROM stickers WHERE discord_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Fetch all stickers for a user (all categories).
    pub async fn get_all_user_stickers(&self, discord_id: u64) -> CoreResult<Vec<Sticker>> {
        let id = discord_id as i64;

        let stickers = sqlx::query_as::<_, Sticker>(
            "SELECT id, discord_id, sticker_url, display_name, category
             FROM stickers WHERE discord_id = ?",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;

        Ok(stickers)
    }

    /// Fetch stickers for a user filtered by category.
    pub async fn get_user_stickers_by_category(
        &self,
        discord_id: u64,
        category: StickerCategory,
    ) -> CoreResult<Vec<Sticker>> {
        let id = discord_id as i64;

        let stickers = sqlx::query_as::<_, Sticker>(
            "SELECT id, discord_id, sticker_url, display_name, category
             FROM stickers WHERE discord_id = ? AND category = ?",
        )
        .bind(id)
        .bind(category)
        .fetch_all(&self.pool)
        .await?;

        Ok(stickers)
    }

    /// Count how many stickers a user has in a specific category.
    pub async fn get_sticker_count_by_category(
        &self,
        discord_id: u64,
        category: StickerCategory,
    ) -> CoreResult<i64> {
        let id = discord_id as i64;

        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM stickers
             WHERE discord_id = ? AND category = ?",
        )
        .bind(id)
        .bind(category)
        .fetch_one(&self.pool)
        .await?;

        Ok(count)
    }

    /// Check whether a user already has a sticker with the given URL.
    pub async fn sticker_url_exists(&self, discord_id: u64, sticker_url: &str) -> CoreResult<bool> {
        let id = discord_id as i64;

        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM stickers
             WHERE discord_id = ? AND sticker_url = ?",
        )
        .bind(id)
        .bind(sticker_url)
        .fetch_one(&self.pool)
        .await?;

        Ok(count > 0)
    }
}
