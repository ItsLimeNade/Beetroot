use serenity::all::{Colour, CreateEmbed, CreateEmbedFooter};

pub fn create_update_embed(version: &str) -> CreateEmbed {
    let changelog = match version {
        "0.1.2" => vec![
            "**⚠️WARNING⚠️**",
            "We recommend closed loops users to remove the threshold (by setting it to 0) they set prior to this update (read changelog)",
            "",
            "**New Features:**",
            "• Added IOB (Insulin On Board) & COB (Carbs On Board) display (appears when using `/bg`)",
            "• Blood glucose unit conversion features (`/convert {value} {unit}`)",
            "• Sticker customization improvements (`/stickers`)",
            "• Analyzing blood glucose values in any mesages using the `Analyzing Units` context menu command",
            "• Thanks to Gar, AAPS users will now benefit from SMB detection instead of using the `/threshold` command, which makes the bot
            easier and more convenient to use by separating manual boluses from automatic ones.",
            "",
            "**Fixes:**",
            "• Help command updates",
            "• Various bug fixes and improvements",
        ],
        _ => vec![
            "**What's New:**",
            "• Bug fixes and performance improvements",
            "• Enhanced stability",
        ],
    };

    CreateEmbed::new()
        .title(format!("🎉 Beetroot has been updated to v{}", version))
        .description("Here's what's new in this update:")
        .color(Colour::DARK_GREEN)
        .field("Changelog", changelog.join("\n"), false)
        .field("For more info","For additional information, please check out the official repository: https://github.com/ItsLimeNade/Beetroot/releases", false)
        .footer(CreateEmbedFooter::new(
            "Thank you for using Beetroot! Use /help to see all available commands.",
        ))
}
