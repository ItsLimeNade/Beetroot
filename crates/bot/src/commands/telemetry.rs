use crate::data::{Context, Error};
use crate::utils::emojis;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{
    ButtonStyle, Colour, CreateActionRow, CreateButton, CreateEmbed, CreateEmbedFooter,
    CreateInteractionResponse, CreateInteractionResponseMessage,
};

/// The exact, explicit description of what telemetry Beetroot records. Shared by
/// `/telemetry` and the `/setup` consent step so the two never drift apart.
/// Kept under Discord's 1024-character embed field limit.
pub const TELEMETRY_DISCLOSURE: &str = "\
**What telemetry records (only if you opt in):**
- Which command you run (e.g. `/bg`, `/graph`) and when
- How long the command took, in milliseconds
- Your Discord user ID, so unique users can be counted

**Telemetry never includes:**
- Your glucose readings or any Nightscout data
- The content of your messages

Your Nightscout URL and access token are stored (the token encrypted) so the \
bot can fetch your data. That is required to run the bot, is never part of \
telemetry, and is never shared. See or erase everything stored about you with \
`/my-data` and `/delete-account`.

Telemetry is used only for aggregate stats: how many people use Beetroot and \
which commands are popular. Change your choice any time with `/telemetry`.";

#[derive(Debug, poise::ChoiceParameter)]
pub enum TelemetryChoice {
    #[name = "Enable (opt in)"]
    Enable,
    #[name = "Disable (opt out)"]
    Disable,
}

/// Opt in or out of usage telemetry.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("telemetry")]
pub async fn telemetry(
    ctx: Context<'_>,
    #[description = "Turn usage telemetry on or off"] action: TelemetryChoice,
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
            "You are now sharing usage telemetry. Thank you, it helps improve Beetroot.",
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

/// Commands that manage telemetry consent themselves, so the one-time notice
/// must not fire before them.
const NOTICE_SKIP_COMMANDS: &[&str] = &["setup", "telemetry"];

/// Reserved `seen_tips` id marking that the one-time telemetry notice was shown.
const NOTICE_SEEN_ID: &str = "telemetry_consent_notice";

/// Show a one-time apology and telemetry choice to a user who predates the
/// opt-in (their choice is still unset).
///
/// Returns `Ok(true)` when the notice was sent, so the caller can skip a tip
/// this time. Sent as the interaction's initial response, exactly like a tip,
/// so the command still delivers its own output as followups.
pub async fn maybe_prompt_existing_user(ctx: Context<'_>) -> Result<bool, Error> {
    if NOTICE_SKIP_COMMANDS.contains(&ctx.command().name.as_str()) {
        return Ok(false);
    }

    let db = &ctx.data().database;
    let user_id = ctx.author().id.get();

    if !db.needs_telemetry_prompt(user_id).await? {
        return Ok(false);
    }
    if db
        .get_seen_tip_ids(user_id)
        .await?
        .iter()
        .any(|id| id == NOTICE_SEEN_ID)
    {
        return Ok(false);
    }

    let embed = CreateEmbed::new()
        .title(format!("{} About telemetry", emojis::tip()))
        .description(
            "Sorry: if you used Beetroot before this choice existed, usage telemetry \
             may have been recorded without asking you. It is now **off** for \
             you unless you turn it on, and you can see anything stored about you with \
             `/my-data`.",
        )
        .field("What this means", TELEMETRY_DISCLOSURE, false)
        .color(Colour::BLURPLE);

    let buttons = CreateActionRow::Buttons(vec![
        CreateButton::new("tel_notice_accept")
            .label("Enable telemetry")
            .style(ButtonStyle::Success),
        CreateButton::new("tel_notice_decline")
            .label("Keep it off")
            .style(ButtonStyle::Secondary),
    ]);

    ctx.send(
        poise::CreateReply::default()
            .embed(embed)
            .components(vec![buttons])
            .ephemeral(true),
    )
    .await?;

    ctx.set_invocation_data(crate::tips::InteractionAcked).await;
    db.mark_tip_seen(user_id, NOTICE_SEEN_ID).await?;

    Ok(true)
}

/// Handle the buttons on the one-time telemetry notice. No-op for any other
/// component. Called from the global interaction event handler.
pub async fn handle_notice_component(
    serenity_ctx: &serenity::Context,
    mci: &serenity::ComponentInteraction,
    db: &beetroot_core::Database,
) {
    let accepted = match mci.data.custom_id.as_str() {
        "tel_notice_accept" => true,
        "tel_notice_decline" => false,
        _ => return,
    };

    let user_id = mci.user.id.get();
    match db.set_telemetry_consent(user_id, accepted).await {
        Ok(()) => tracing::info!(
            user = %crate::logging::redact(user_id),
            accepted,
            "telemetry consent set from one-time notice"
        ),
        Err(e) => tracing::warn!(error = %e, "failed to record telemetry choice from notice"),
    }

    let (title, body, color) = if accepted {
        (
            format!("{} Telemetry Enabled", emojis::wifi()),
            "Thank you. Change this any time with `/telemetry`.",
            Colour::DARK_GREEN,
        )
    } else {
        (
            format!("{} Telemetry Off", emojis::lock_closed()),
            "Kept off. You can enable it any time with `/telemetry`.",
            Colour::DARK_GREY,
        )
    };

    let embed = CreateEmbed::new().title(title).description(body).color(color);

    let _ = mci
        .create_response(
            serenity_ctx,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .embed(embed)
                    .components(vec![]),
            ),
        )
        .await;
}
