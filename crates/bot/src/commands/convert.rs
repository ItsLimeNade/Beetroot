use crate::data::{Context, Error};
use crate::utils::emojis;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateEmbed};

/// Convert blood glucose units between mg/dL and mmol/L.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("convert")]
pub async fn convert(
    ctx: Context<'_>,
    #[description = "The glucose value to convert"] value: f64,
    #[description = "Choose the conversion type"] unit: ConversionChoice,
) -> Result<(), Error> {
    let (result, from_unit, to_unit) = match unit {
        ConversionChoice::MgdlToMmol => (value / 18.0, "mg/dL", "mmol/L"),
        ConversionChoice::MmolToMgdl => (value * 18.0, "mmol/L", "mg/dL"),
    };

    let embed = CreateEmbed::new()
        .title(format!("{} Blood Glucose Conversion", emojis::sugar()))
        .description(format!(
            "**{:.1} {}** = **{:.1} {}**",
            value, from_unit, result, to_unit
        ))
        .color(Colour::BLUE);

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}

#[derive(Debug, Clone, Copy, poise::ChoiceParameter)]
pub enum ConversionChoice {
    #[name = "to mmol/L"]
    MgdlToMmol,
    #[name = "to mg/dL"]
    MmolToMgdl,
}
