use crate::data::{Context, Error};
use crate::onboarding::{self, Start};
use macros::track_analytics;

/// Connect your Nightscout to Beetroot, with a quick tour of what it can do
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("setup")]
pub async fn setup(ctx: Context<'_>) -> Result<(), Error> {
    onboarding::run(ctx, Start::Setup).await
}
