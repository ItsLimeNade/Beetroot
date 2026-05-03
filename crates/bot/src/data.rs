use beetroot_core::Database;

/// Data struct to share global information across all the bot's codebase.
pub struct Data {
    pub database: Database,
}

/// Global Error type used to handle all errors.
pub type Error = anyhow::Error;

/// Poise context used to handle commands.
pub type Context<'a> = poise::Context<'a, Data, Error>;
