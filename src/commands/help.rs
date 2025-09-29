use crate::Handler;
use serenity::all::{
    ButtonStyle, Colour, CommandInteraction, CommandOptionType, ComponentInteraction, Context, CreateActionRow, CreateButton,
    CreateEmbed, CreateInteractionResponse, CreateInteractionResponseMessage, InteractionContext,
    ResolvedOption, ResolvedValue,
};
use serenity::builder::{CreateCommand, CreateCommandOption};

pub async fn run(
    _handler: &Handler,
    context: &Context,
    interaction: &CommandInteraction,
) -> anyhow::Result<()> {
    let mut page = 1;

    for option in &interaction.data.options() {
        if let ResolvedOption {
            name: "page",
            value: ResolvedValue::Integer(p),
            ..
        } = option
        {
            page = *p as u8;
        }
    }

    let (embed, components) = create_help_page(page);

    let mut response = CreateInteractionResponseMessage::new()
        .embed(embed)
        .ephemeral(true);

    if let Some(action_row) = components {
        response = response.components(vec![action_row]);
    }

    interaction
        .create_response(context, CreateInteractionResponse::Message(response))
        .await?;

    Ok(())
}

fn create_help_page(page: u8) -> (CreateEmbed, Option<CreateActionRow>) {
    let total_pages = 2;
    let page = page.clamp(1, total_pages);

    let embed = match page {
        1 => CreateEmbed::new()
            .title("Beetroot Commands (Page 1/2)")
            .description("Here are the main commands:")
            .color(Colour::from_rgb(34, 197, 94))
            .field(
                "/bg [user]",
                "Get current blood glucose reading. Optionally specify a user to view their data (requires permission).",
                false,
            )
            .field(
                "/graph [hours] [user]",
                "Generate a blood glucose graph. Specify hours (3-24) and optionally a user to view their graph (requires permission).",
                false,
            )
            .field(
                "/setup",
                "Configure your Nightscout URL and privacy settings. Required before using other commands.",
                false,
            )
            .field(
                "/token",
                "Set or update your Nightscout API token for authentication (optional but recommended).",
                false,
            ),
        2 => CreateEmbed::new()
            .title("Beetroot Commands (Page 2/2)")
            .description("Additional commands and settings:")
            .color(Colour::from_rgb(34, 197, 94))
            .field(
                "/allow @user [action]",
                "Manage who can view your blood glucose data. Add or remove users from your allowed list.",
                false,
            )
            .field(
                "/set-threshold value [display]",
                "Configure microbolus threshold (in units) and whether to display them on graphs. Doses ≤ threshold are considered microbolus.",
                false,
            )
            .field(
                "/help [page]",
                "Show this help message with all available commands. Use page parameter to navigate.",
                false,
            )
            .field(
                "/info",
                "Show information about Beetroot bot, GitHub repository, and how to report issues.",
                false,
            ),
        _ => unreachable!(),
    };

    let embed = embed.footer(serenity::all::CreateEmbedFooter::new(
        "Use /info for bot information and GitHub repository"
    ));

    let components = if total_pages > 1 {
        let mut buttons = Vec::new();

        if page > 1 {
            buttons.push(CreateButton::new(format!("help_page_{}", page - 1))
                .label("◀ Previous")
                .style(ButtonStyle::Secondary));
        }

        if page < total_pages {
            buttons.push(CreateButton::new(format!("help_page_{}", page + 1))
                .label("Next ▶")
                .style(ButtonStyle::Secondary));
        }

        if !buttons.is_empty() {
            Some(CreateActionRow::Buttons(buttons))
        } else {
            None
        }
    } else {
        None
    };

    (embed, components)
}

pub async fn handle_button(
    _handler: &Handler,
    context: &Context,
    interaction: &ComponentInteraction,
) -> anyhow::Result<()> {
    let custom_id = &interaction.data.custom_id;

    if let Some(page_str) = custom_id.strip_prefix("help_page_") {
        let page: u8 = page_str.parse().unwrap_or(1);
        let (embed, components) = create_help_page(page);

        let mut response = CreateInteractionResponseMessage::new()
            .embed(embed);

        if let Some(action_row) = components {
            response = response.components(vec![action_row]);
        }

        interaction
            .create_response(context, CreateInteractionResponse::Message(response))
            .await?;
    }

    Ok(())
}

pub fn register() -> CreateCommand {
    CreateCommand::new("help")
        .description("Show all available commands and their usage")
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::Integer,
                "page",
                "Page number to view (1-2)"
            )
            .min_int_value(1)
            .max_int_value(2)
            .required(false),
        )
        .contexts(vec![
            InteractionContext::Guild,
            InteractionContext::PrivateChannel,
        ])
}