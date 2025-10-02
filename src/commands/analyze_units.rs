use crate::Handler;
use regex::Regex;
use serenity::all::{
    Colour, CommandInteraction, Context, CreateEmbed, CreateInteractionResponse,
    CreateInteractionResponseMessage, InteractionContext,
};
use serenity::builder::CreateCommand;
use serenity::model::application::CommandType;

#[derive(Debug)]
struct UnitConversion {
    original: String,
    value: f64,
    unit: String,
    converted_value: f64,
    converted_unit: String,
}

pub async fn run(
    _handler: &Handler,
    context: &Context,
    interaction: &CommandInteraction,
) -> anyhow::Result<()> {
    let resolved = &interaction.data.resolved;
    let target_message = if let Some(message) = resolved.messages.values().next() {
        message
    } else {
        crate::commands::error::run(
            context,
            interaction,
            "No message found in context menu interaction.",
        )
        .await?;
        return Ok(());
    };

    let content = &target_message.content;
    let conversions = detect_glucose_units(content);

    if conversions.is_empty() {
        let embed = CreateEmbed::new()
            .title("No Blood Glucose Units Found")
            .description("No diabetes units (mg/dL or mmol/L) were detected in this message.")
            .color(Colour::ORANGE);

        let response = CreateInteractionResponseMessage::new()
            .embed(embed)
            .ephemeral(true);

        interaction
            .create_response(&context.http, CreateInteractionResponse::Message(response))
            .await?;
        return Ok(());
    }

    let conversion_list: String = conversions
        .iter()
        .map(|c| {
            format!(
                "• **{}** → **{:.1} {}**",
                c.original, c.converted_value, c.converted_unit
            )
        })
        .collect::<Vec<String>>()
        .join("\n");

    let embed = CreateEmbed::new()
        .title("Blood Glucose Unit Conversions")
        .description(format!(
            "Found {} conversion(s):\n\n{}",
            conversions.len(),
            conversion_list
        ))
        .color(Colour::BLUE)
        .footer(serenity::all::CreateEmbedFooter::new(
            "Conversions detected from the message",
        ));

    let response = CreateInteractionResponseMessage::new()
        .embed(embed)
        .ephemeral(true);

    interaction
        .create_response(&context.http, CreateInteractionResponse::Message(response))
        .await?;

    Ok(())
}

fn detect_glucose_units(content: &str) -> Vec<UnitConversion> {
    let mut conversions = Vec::new();

    let patterns = [
        r"(\d+(?:\.\d+)?)\s*(?:mg/dl|mg/dL|mgdl|MGDL|MG/DL)",
        r"(\d+(?:\.\d+)?)\s*(?:mmol/l|mmol/L|mmoll|MMOL/L|MMOLL)",
        r"(\d+(?:\.\d+)?)\s*mg\b",
        r"(\d+(?:\.\d+)?)\s*mmol\b",
    ];

    for (idx, pattern_str) in patterns.iter().enumerate() {
        let re = Regex::new(pattern_str).unwrap();

        for cap in re.captures_iter(content) {
            if let Some(value_match) = cap.get(1) {
                let value: f64 = value_match.as_str().parse().unwrap_or(0.0);

                let (unit, converted_value, converted_unit) = if idx == 0 || idx == 2 {
                    ("mg/dL", value / 18.0, "mmol/L")
                } else {
                    ("mmol/L", value * 18.0, "mg/dL")
                };

                let is_valid = match unit {
                    "mg/dL" => (20.0..=600.0).contains(&value),
                    "mmol/L" => (1.0..=35.0).contains(&value),
                    _ => false,
                };

                if is_valid {
                    conversions.push(UnitConversion {
                        original: cap.get(0).unwrap().as_str().to_string(),
                        value,
                        unit: unit.to_string(),
                        converted_value,
                        converted_unit: converted_unit.to_string(),
                    });
                }
            }
        }
    }

    conversions.sort_by(|a, b| a.original.cmp(&b.original));
    conversions.dedup_by(|a, b| a.original == b.original);

    conversions
}

pub fn register() -> CreateCommand {
    CreateCommand::new("Analyze Units")
        .kind(CommandType::Message)
        .contexts(vec![
            InteractionContext::Guild,
            InteractionContext::PrivateChannel,
        ])
}
