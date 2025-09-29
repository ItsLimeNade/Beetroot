use crate::Handler;
use serenity::all::{
    CommandInteraction, CommandOptionType, Context, CreateInteractionResponse,
    CreateInteractionResponseMessage, InteractionContext, ResolvedOption, ResolvedValue,
};
use serenity::builder::{CreateCommand, CreateCommandOption};

pub async fn run(
    handler: &Handler,
    context: &Context,
    interaction: &CommandInteraction,
) -> anyhow::Result<()> {
    let mut threshold: Option<f32> = None;
    let mut display: Option<bool> = None;

    for option in &interaction.data.options() {
        match option {
            ResolvedOption {
                name: "value",
                value: ResolvedValue::Number(val),
                ..
            } => {
                threshold = Some(*val as f32);
            }
            ResolvedOption {
                name: "display",
                value: ResolvedValue::Boolean(val),
                ..
            } => {
                display = Some(*val);
            }
            _ => {}
        }
    }

    let threshold = threshold.ok_or_else(|| anyhow::anyhow!("Threshold value is required"))?;
    let display = display.unwrap_or(true);

    if threshold < 0.0 || threshold > 100.0 {
        let response = CreateInteractionResponseMessage::new()
            .content("[ERROR] Threshold must be between 0 and 100 units.")
            .ephemeral(true);
        interaction
            .create_response(context, CreateInteractionResponse::Message(response))
            .await?;
        return Ok(());
    }

    handler
        .database
        .update_microbolus_settings(interaction.user.id.get(), threshold, display)
        .await?;

    let response_content = format!(
        "✅ **Microbolus threshold updated!**\n\n🎯 **Threshold:** {:.1} units\n📊 **Display on graph:** {}\n\n💡 Insulin doses {} {:.1}u will be considered microbolus injections.",
        threshold,
        if display { "Yes" } else { "No" },
        if threshold > 0.0 { "≤" } else { "<" },
        threshold
    );

    let response = CreateInteractionResponseMessage::new()
        .content(response_content)
        .ephemeral(true);

    interaction
        .create_response(context, CreateInteractionResponse::Message(response))
        .await?;

    Ok(())
}

pub fn register() -> CreateCommand {
    CreateCommand::new("set-threshold")
        .description("Set microbolus threshold and display preferences")
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::Number,
                "value",
                "Threshold value in units (doses ≤ this value are microbolus)"
            )
            .min_number_value(0.0)
            .max_number_value(100.0)
            .required(true),
        )
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::Boolean,
                "display",
                "Whether to display microbolus on graph (default: true)"
            )
            .required(false),
        )
        .contexts(vec![
            InteractionContext::Guild,
            InteractionContext::PrivateChannel,
        ])
}