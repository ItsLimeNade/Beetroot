use serenity::all::{Colour, CreateEmbed, CreateEmbedFooter};

pub fn create_update_embed(version: &str) -> CreateEmbed {
    let changelog = match version {
        "0.1.2" => vec![
            "**New Features:**",
            "• Added IOB (Insulin On Board) & COB (Carbs On Board) display (appears when using `/bg`",
            "• Blood glucose unit conversion features (`/convert {value} {unit}`)",
            "• Sticker customization improvements (`/stickers`)",
            "• Analyzing blood glucose values in any mesages using the `Analyzing Units` context menu command",
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
        .color(Colour::from_rgb(252, 186, 0))
        .field("Changelog", changelog.join("\n"), false)
        .footer(CreateEmbedFooter::new(
            "Thank you for using Beetroot! Use /help to see all available commands.",
        ))
}
