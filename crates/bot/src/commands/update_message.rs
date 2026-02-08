use serenity::all::{Colour, CreateEmbed, CreateEmbedFooter};

pub fn create_update_embed(version: &str) -> CreateEmbed {
    match version {
        "0.2.1" => {
            let whats_new = [
                "• **Doubled** graph resolution for bigger and clearer images",
                "• Added warning in `/bg` if data is older than 15 min",
                "• Added contextual stickers that generate based on your blood glucose value",
                "• Updated `/stickers` command to work with contextual stickers",
                "• Added `/set-token`, `/set-nightscout-url`, `/get-nightscout-url` and `/set-visibility` commands",
                "• MBG (meter blood glucose) entries now displayed as fingerprick readings on graphs",
                "• Target ranges now dynamically fetched from your Nightscout profile",
                "• Added faint striped lines at target high/low ranges on graphs",
                "• `/bg` now uses custom title from Nightscout status settings",
                "• `/bg` displays fingerprick values from past 30 min in both mg/dL and mmol/L",
            ];

            let fixes = [
                "• Fixed missing data on graph edges collapsing the graph",
                "• Fixed MBG entries not being fetched from the API",
                "• Fixed duplicate detection treating MBG and SGV entries the same",
                "• Fixed incorrect thresholds fetching.",
            ];

            CreateEmbed::new()
                .title(format!(
                    "🎉 Beetroot has been updated to v{} | Enhancements Update",
                    version
                ))
                .description("Here's what's new in this update:")
                .color(Colour::DARK_GREEN)
                .field("What's New", whats_new.join("\n"), false)
                .field("Fixes", fixes.join("\n"), false)
                .field(
                    "For more info",
                    "Check out: https://github.com/ItsLimeNade/Beetroot/releases",
                    false,
                )
                .footer(CreateEmbedFooter::new(
                    "Thank you for using Beetroot! Use /help to see all available commands.",
                ))
        }
        _ => CreateEmbed::new()
            .title(format!("🎉 Beetroot has been updated to v{}", version))
            .description("Here's what's new in this update:")
            .color(Colour::DARK_GREEN)
            .field(
                "What's New",
                "• Bug fixes and performance improvements\n• Enhanced stability",
                false,
            )
            .field(
                "For more info",
                "Check out: https://github.com/ItsLimeNade/Beetroot/releases",
                false,
            )
            .footer(CreateEmbedFooter::new(
                "Thank you for using Beetroot! Use /help to see all available commands.",
            )),
    }
}
