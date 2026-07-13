use crate::data::{Context, Error};
use crate::utils::emojis;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use regex::Regex;
use serenity::all::{Colour, CreateEmbed, CreateEmbedFooter};
use std::sync::LazyLock;

/// A single glucose reading detected in a message and its converted value.
#[derive(Debug)]
struct UnitConversion {
    /// The matched text as it appeared in the message, e.g. "120 mg/dL".
    original: String,
    converted_value: f64,
    converted_unit: &'static str,
}

/// Detection patterns paired with whether a match is mg/dL (`true`) or mmol/L
/// (`false`). Compiled once on first use.
static GLUCOSE_PATTERNS: LazyLock<[(Regex, bool); 4]> = LazyLock::new(|| {
    [
        (
            Regex::new(r"(\d+(?:\.\d+)?)\s*(?:mg/dl|mg/dL|mgdl|MGDL|MG/DL)").unwrap(),
            true,
        ),
        (
            Regex::new(r"(\d+(?:\.\d+)?)\s*(?:mmol/l|mmol/L|mmoll|MMOL/L|MMOLL)").unwrap(),
            false,
        ),
        (Regex::new(r"(\d+(?:\.\d+)?)\s*mg\b").unwrap(), true),
        (Regex::new(r"(\d+(?:\.\d+)?)\s*mmol\b").unwrap(), false),
    ]
});

/// Scan a message for blood glucose units and convert them between mg/dL and mmol/L.
#[poise::command(
    context_menu_command = "Analyze Units",
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("analyze_units")]
pub async fn analyze_units(
    ctx: Context<'_>,
    #[description = "Message to scan for glucose units"] message: serenity::Message,
) -> Result<(), Error> {
    let conversions = detect_glucose_units(&message.content);

    if conversions.is_empty() {
        let embed = CreateEmbed::new()
            .title(format!(
                "{} No Blood Glucose Units Found",
                emojis::warning()
            ))
            .description("No diabetes units (mg/dL or mmol/L) were detected in this message.")
            .color(Colour::ORANGE);

        ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
            .await?;
        return Ok(());
    }

    let conversion_list = conversions
        .iter()
        .map(|c| {
            format!(
                "• **{}** → **{:.1} {}**",
                c.original, c.converted_value, c.converted_unit
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let embed = CreateEmbed::new()
        .title(format!("{} Blood Glucose Unit Conversions", emojis::sugar()))
        .description(format!(
            "Found {} conversion(s):\n\n{}",
            conversions.len(),
            conversion_list
        ))
        .color(Colour::BLUE)
        .footer(CreateEmbedFooter::new(
            "Conversions detected from the message",
        ));

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}

/// Find plausible glucose readings in `content` and convert each to the other unit.
///
/// Values outside a sane physiological range (mg/dL 20-600, mmol/L 1-35) are
/// ignored so ordinary numbers in a sentence aren't mistaken for readings.
fn detect_glucose_units(content: &str) -> Vec<UnitConversion> {
    let mut conversions = Vec::new();

    for (re, is_mgdl) in GLUCOSE_PATTERNS.iter() {
        for cap in re.captures_iter(content) {
            let Some(value_match) = cap.get(1) else {
                continue;
            };
            let value: f64 = value_match.as_str().parse().unwrap_or(0.0);

            let (converted_value, converted_unit, in_range) = if *is_mgdl {
                (value / 18.0, "mmol/L", (20.0..=600.0).contains(&value))
            } else {
                (value * 18.0, "mg/dL", (1.0..=35.0).contains(&value))
            };

            if in_range {
                conversions.push(UnitConversion {
                    original: cap.get(0).unwrap().as_str().to_string(),
                    converted_value,
                    converted_unit,
                });
            }
        }
    }

    conversions.sort_by(|a, b| a.original.cmp(&b.original));
    conversions.dedup_by(|a, b| a.original == b.original);

    conversions
}
