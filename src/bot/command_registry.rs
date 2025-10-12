use crate::commands;
use serenity::all::CreateCommand;

/// Get all slash commands and context menu commands to register
pub fn get_all_commands() -> Vec<CreateCommand> {
    vec![
        // Slash commands
        commands::allow::register(),
        commands::bg::register(),
        commands::convert::register(),
        commands::get_nightscout_url::register(),
        commands::graph::register(),
        commands::help::register(),
        commands::info::register(),
        commands::set_nightscout_url::register(),
        commands::set_threshold::register(),
        commands::set_token::register(),
        commands::set_visibility::register(),
        commands::setup::register(),
        commands::stickers::register(),
        commands::token::register(),
        // Context menu commands
        commands::add_sticker::register(),
        commands::analyze_units::register(),
    ]
}
