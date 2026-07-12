use crate::data::{Context, Error};
use crate::utils::emojis;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateEmbed, CreateEmbedFooter};

/// The exact, explicit description of what telemetry Beetroot records. Shared by
/// `/telemetry` and the `/setup` consent step so the two never drift apart.
/// Kept under Discord's 1024-character embed field limit.
pub const TELEMETRY_DISCLOSURE: &str = "\
**What is recorded (only if you opt in):**
- Which command you run (e.g. `/bg`, `/graph`) and the time it ran
- How long the command took, in milliseconds
- Your Discord user ID, so unique users can be counted

**What is never recorded:**
- Your glucose readings or any Nightscout data
- Your Nightscout URL or access token
- The content of your messages

It is used only for aggregate stats: how many people use Beetroot, which \
commands are popular, and performance. You can change your mind any time with \
`/telemetry`, see everything stored about you with `/my-data`, and erase it with \
`/delete-account`.";

#[derive(Debug, poise::ChoiceParameter)]
pub enum TelemetryChoice {
    #[name = "Enable (opt in)"]
    Enable,
    #[name = "Disable (opt out)"]
    Disable,
}

/// Opt in or out of anonymous usage telemetry.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("telemetry")]
pub async fn telemetry(
    ctx: Context<'_>,
    #[description = "Turn anonymous usage telemetry on or off"] action: TelemetryChoice,
) -> Result<(), Error> {
    let user_id = ctx.author().id.get();
    let db = &ctx.data().database;

    let accepted = matches!(action, TelemetryChoice::Enable);
    db.set_telemetry_consent(user_id, accepted).await?;

    tracing::info!(
        user = %crate::logging::redact(user_id),
        accepted,
        "telemetry consent updated"
    );

    let (title, summary, color) = if accepted {
        (
            format!("{} Telemetry Enabled", emojis::wifi()),
            "You are now sharing anonymous usage telemetry. Thank you, it helps improve Beetroot.",
            Colour::DARK_GREEN,
        )
    } else {
        (
            format!("{} Telemetry Disabled", emojis::lock_closed()),
            "Telemetry is off. Nothing about your command usage will be recorded from now on.",
            Colour::DARK_GREY,
        )
    };

    let embed = CreateEmbed::new()
        .title(title)
        .description(summary)
        .field("Details", TELEMETRY_DISCLOSURE, false)
        .footer(CreateEmbedFooter::new(
            "See what is stored about you with /my-data",
        ))
        .color(color);

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}
