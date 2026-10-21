use crate::data::{Context, Error};
use crate::onboarding::{self, Start};
use macros::track_analytics;

/// See what Beetroot can do, and get help connecting your Nightscout
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("help")]
pub async fn help(ctx: Context<'_>) -> Result<(), Error> {
    onboarding::run(ctx, Start::Help).await
}
