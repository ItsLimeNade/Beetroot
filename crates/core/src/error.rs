#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("Crypto error: {0}")]
    Crypto(String),

    #[error("User not found")]
    UserNotFound,

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

pub type CoreResult<T> = Result<T, CoreError>;
