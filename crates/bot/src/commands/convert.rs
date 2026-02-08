use crate::bot::Handler;
use serenity::all::{
    Colour, CommandInteraction, CommandOptionType, Context, CreateEmbed, CreateInteractionResponse,
    CreateInteractionResponseMessage, InteractionContext, ResolvedOption, ResolvedValue,
};
use serenity::builder::{CreateCommand, CreateCommandOption};

pub async fn run(
    _handler: &Handler,
    context: &Context,
    interaction: &CommandInteraction,
) -> anyhow::Result<()> {
    let mut unit: Option<String> = None;
    let mut value: Option<f64> = None;

    for option in &interaction.data.options() {
        match option {
            ResolvedOption {
                name: "value",
                value: ResolvedValue::Number(v),
                ..
            } => {
                value = Some(*v);
            }
            ResolvedOption {
                name: "unit",
                value: ResolvedValue::String(u),
                ..
            } => {
                unit = Some(u.to_string());
            }
            _ => {}
        }
    }

    let value = if let Some(v) = value {
        v
    } else {
        crate::commands::error::run(context, interaction, "Please provide a value to convert.")
            .await?;
        return Ok(());
    };

    let unit = if let Some(u) = unit {
        u
    } else {
        crate::commands::error::run(context, interaction, "Please specify the unit.").await?;
        return Ok(());
    };

    let (result, from_unit, to_unit) = match unit.as_str() {
        "mgdl_to_mmol" => (value / 18.0, "mg/dL", "mmol/L"),
        "mmol_to_mgdl" => (value * 18.0, "mmol/L", "mg/dL"),
        _ => {
            crate::commands::error::run(context, interaction, "Invalid conversion type.").await?;
            return Ok(());
        }
    };

    let embed = CreateEmbed::new()
        .title("Blood Glucose Conversion")
        .description(format!(
            "**{:.1} {}** = **{:.1} {}**",
            value, from_unit, result, to_unit
        ))
        .color(Colour::BLUE);

    let response = CreateInteractionResponseMessage::new()
        .embed(embed)
        .ephemeral(true);

    interaction
        .create_response(&context.http, CreateInteractionResponse::Message(response))
        .await?;

    Ok(())
}

pub fn register() -> CreateCommand {
    CreateCommand::new("convert")
        .description("Convert blood glucose units between mg/dL and mmol/L")
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::Number,
                "value",
                "The glucose value to convert",
            )
            .required(true),
        )
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::String,
                "unit",
                "Choose the conversion type",
            )
            .required(true)
            .add_string_choice("to mmol/L", "mgdl_to_mmol")
            .add_string_choice("to mg/dL", "mmol_to_mgdl"),
        )
        .contexts(vec![
            InteractionContext::Guild,
            InteractionContext::PrivateChannel,
        ])
}
