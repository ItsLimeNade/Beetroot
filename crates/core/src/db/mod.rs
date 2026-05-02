mod analytics;
mod session;
mod sticker;
mod user;

pub use user::TokenUpdate;

use crate::error::CoreResult;
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;
use std::str::FromStr;

/// Cloneable handle on the application database.
///
/// `SqlitePool` is internally reference-counted, so cloning a `Database`
/// is cheap and shares the same underlying pool.
#[derive(Clone, Debug)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Connect to SQLite, create the file if missing, and run migrations.
    ///
    /// This is the main entry point for production use.
    pub async fn connect(database_url: &str) -> CoreResult<Self> {
        let options = SqliteConnectOptions::from_str(database_url)
            .map_err(crate::error::CoreError::Database)?
            .create_if_missing(true);

        let pool = SqlitePool::connect_with(options).await?;
        sqlx::migrate!().run(&pool).await?;

        Ok(Self { pool })
    }

    /// Wrap an existing, already-migrated pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Borrow the underlying pool.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn wraps_a_pool() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("create in-memory sqlite");

        let db = Database::new(pool);
        let db2 = db.clone();

        let _: i64 = sqlx::query_scalar("SELECT 1")
            .fetch_one(db.pool())
            .await
            .expect("first query");
        let _: i64 = sqlx::query_scalar("SELECT 2")
            .fetch_one(db2.pool())
            .await
            .expect("second query");
    }
}
