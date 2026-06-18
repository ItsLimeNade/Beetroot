use crate::data::{Context, Error};
use crate::utils::emojis;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateEmbed};

/// How treatments (carbs/insulin) are positioned on `/graph`.
#[derive(Debug, Clone, Copy, poise::ChoiceParameter)]
pub enum TreatmentModeChoice {
    #[name = "Contextual (attached to readings)"]
    Contextual,
    #[name = "Timeline (lanes along the bottom)"]
    Timeline,
}

impl TreatmentModeChoice {
    /// The value stored in the database.
    fn as_db(self) -> &'static str {
        match self {
            Self::Contextual => "contextual",
            Self::Timeline => "timeline",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Contextual => "Contextual",
            Self::Timeline => "Timeline",
        }
    }

    fn body(self) -> &'static str {
        match self {
            Self::Contextual => "Carbs and insulin are attached to the nearest reading.",
            Self::Timeline => "Carbs and insulin are shown in lanes along the bottom.",
        }
    }
}

/// Choose how treatments are shown on your /graph.
#[poise::command(
    slash_command,
    rename = "treatment-mode",
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("treatment_mode")]
pub async fn treatment_mode(
    ctx: Context<'_>,
    #[description = "How treatments are positioned on the graph"] mode: TreatmentModeChoice,
) -> Result<(), Error> {
    let user_id = ctx.author().id.get();
    let _ = get_db_user!(ctx, user_id);

    let db = &ctx.data().database;
    db.set_treatment_mode(user_id, mode.as_db()).await?;
    tracing::info!(
        user = %crate::logging::redact(user_id),
        mode = mode.as_db(),
        "treatment mode updated"
    );

    let embed = CreateEmbed::new()
        .title(format!("{} Treatment Display", emojis::micro_bolus()))
        .description(format!("**{}**\n{}", mode.label(), mode.body()))
        .color(Colour::DARK_GREEN);

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}
