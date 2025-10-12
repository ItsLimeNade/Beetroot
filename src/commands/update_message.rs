use serenity::all::{Colour, CreateEmbed, CreateEmbedFooter};

pub fn create_update_embed(version: &str) -> CreateEmbed {
    let changelog = match version {
        "0.2.0" => vec![
            "**What's new:**",
            "• **Doubled** the graph resolution allowing for noticeably bigger and clearer resulting images",
            "• Added a warning in the `/bg` command if the data is older than 15 min",
            "• Added contextual stickers. When adding a new sticker it will prompt you to categorize it. The sticker will now generate Depending your blood glucose value!",
            "• Updated the `/stickers` commmand to work with contextual stickers",
            "• Added `/set-token`, `/set-nightscout-url`, `/get-nightscout-url` and `/set-visibility` commands to avoid having to run `/setup` each time to change their values.",
            "",
            "**Fixes:**",
            "• Fixed issue where missing data on the edges of the graph would collapse the graph instead of showing the gap",
        ],
        _ => vec![
            "**What's New:**",
            "• Bug fixes and performance improvements",
            "• Enhanced stability",
        ],
    };

    CreateEmbed::new()
        .title(format!("🎉 Beetroot has been updated to v{} | Enhancements Update", version))
        .description("Here's what's new in this update:")
        .color(Colour::DARK_GREEN)
        .field("Changelog", changelog.join("\n"), false)
        .field("For more info","For additional information, please check out the official repository: https://github.com/ItsLimeNade/Beetroot/releases", false)
        .footer(CreateEmbedFooter::new(
            "Thank you for using Beetroot! Use /help to see all available commands.",
        ))
}
