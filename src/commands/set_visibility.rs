use crate::Handler;
use serenity::all::{
    Colour, CommandInteraction, CommandOptionType, Context, CreateCommand, CreateCommandOption,
    CreateEmbed, CreateInteractionResponse, CreateInteractionResponseMessage, InteractionContext,
    ResolvedOption, ResolvedValue,
};

pub async fn run(
    handler: &Handler,
    context: &Context,
    interaction: &CommandInteraction,
) -> anyhow::Result<()> {
    if !handler
        .database
        .user_exists(interaction.user.id.get())
        .await?
    {
        crate::commands::error::run(
            context,
            interaction,
            "You need to run `/setup` first to configure your Nightscout before changing visibility settings.",
        )
        .await?;
        return Ok(());
    }

    let mut visibility: Option<&str> = None;

    for option in &interaction.data.options() {
        if let ResolvedOption {
            name: "visibility",
            value: ResolvedValue::String(vis),
            ..
        } = option
        {
            visibility = Some(vis);
        }
    }

    let visibility =
        visibility.ok_or_else(|| anyhow::anyhow!("Visibility parameter is required"))?;

    let is_private = match visibility {
        "public" => false,
        "private" => true,
        _ => {
            crate::commands::error::run(
                context,
                interaction,
                "Invalid visibility option. Use 'public' or 'private'.",
            )
            .await?;
            return Ok(());
        }
    };

    let current_user_info = handler
        .database
        .get_user_info(interaction.user.id.get())
        .await?;

    // Check if the visibility is already set to the requested value
    if current_user_info.nightscout.is_private == is_private {
        let status = if is_private { "private" } else { "public" };
        crate::commands::error::run(
            context,
            interaction,
            &format!("Your profile is already set to {}.", status),
        )
        .await?;
        return Ok(());
    }

    let updated_nightscout_info = crate::utils::database::NightscoutInfo {
        nightscout_url: current_user_info.nightscout.nightscout_url,
        nightscout_token: current_user_info.nightscout.nightscout_token,
        is_private,
        allowed_people: current_user_info.nightscout.allowed_people,
        microbolus_threshold: current_user_info.nightscout.microbolus_threshold,
        display_microbolus: current_user_info.nightscout.display_microbolus,
    };

    let user_id = interaction.user.id.get();
    match handler
        .database
        .update_user(user_id, updated_nightscout_info)
        .await
    {
        Ok(_) => {
            let (title, description, color) = if is_private {
                (
                    "Profile Set to Private",
                    "Your profile is now **private**.\n\nOnly you and users you've explicitly allowed with `/allow` can view your blood glucose data.",
                    Colour::from_rgb(59, 130, 246), // Blue
                )
            } else {
                (
                    "Profile Set to Public",
                    "Your profile is now **public**.\n\nAnyone can view your blood glucose data. Users in your `/allow` list will still have access.",
                    Colour::from_rgb(34, 197, 94), // Green
                )
            };

            let embed = CreateEmbed::new()
                .title(title)
                .description(description)
                .color(color);

            let response = CreateInteractionResponseMessage::new()
                .embed(embed)
                .ephemeral(true);

            interaction
                .create_response(context, CreateInteractionResponse::Message(response))
                .await?;
        }
        Err(e) => {
            eprintln!("Failed to update visibility: {}", e);
            crate::commands::error::run(
                context,
                interaction,
                "[ERROR] Failed to update your visibility settings. Please try again later.",
            )
            .await?;
        }
    }

    Ok(())
}

pub fn register() -> CreateCommand {
    CreateCommand::new("set-visibility")
        .description("Set whether your profile is public or private")
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::String,
                "visibility",
                "Choose public or private visibility",
            )
            .add_string_choice("Public - Anyone can view", "public")
            .add_string_choice("Private - Only allowed users", "private")
            .required(true),
        )
        .contexts(vec![
            InteractionContext::Guild,
            InteractionContext::PrivateChannel,
        ])
}
