use crate::data::{Context, Error};
use crate::utils::denoise::Strength;
use crate::utils::emojis;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateEmbed};

#[derive(Debug, Clone, Copy, poise::ChoiceParameter)]
pub enum DenoiseChoice {
    #[name = "Off (raw readings)"]
    Off,
    #[name = "Light"]
    Light,
    #[name = "Medium"]
    Medium,
    #[name = "Strong"]
    Strong,
}

impl DenoiseChoice {
    /// The smoothing to apply, or `None` for raw readings.
    pub fn strength(self) -> Option<Strength> {
        match self {
            Self::Off => None,
            Self::Light => Some(Strength::Light),
            Self::Medium => Some(Strength::Medium),
            Self::Strong => Some(Strength::Strong),
        }
    }
}

/// Smooth sensor noise out of the glucose readings on your graphs.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("denoise")]
pub async fn denoise(
    ctx: Context<'_>,
    #[description = "How much to smooth the readings on your graphs"] strength: DenoiseChoice,
) -> Result<(), Error> {
    let user_id = ctx.author().id.get();
    let _ = get_db_user!(ctx, user_id);

    let strength = strength.strength();
    let level = strength.map_or(0, Strength::level);

    let db = &ctx.data().database;
    db.set_graph_denoise(user_id, level).await?;
    tracing::info!(
        user = %crate::logging::redact(user_id),
        level,
        "graph denoise updated"
    );

    let (label, body) = match strength {
        None => (
            "Off",
            "Your graphs will show your sensor's readings exactly as reported.".to_string(),
        ),
        Some(strength) => (
            strength.label(),
            "Your graphs will now smooth out sensor jitter, so the trace follows where \
             your glucose was heading instead of every wobble.\n\n\
             Smoothing rounds off short spikes and dips, so a brief low or high can look \
             milder than your sensor reported. Only the picture changes: `/bg`, `/tir` and \
             `/search` keep using your raw readings."
                .to_string(),
        ),
    };

    let embed = CreateEmbed::new()
        .title(format!("{} Graph Denoising", emojis::image_mode()))
        .description(format!("**{}**\n{}", label, body))
        .color(Colour::DARK_GREEN);

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}
