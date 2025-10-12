use crate::bot::Handler;
use crate::commands;
use anyhow::Result;
use serenity::all::{ComponentInteraction, Context};

/// Route component interactions (button clicks) to their handlers
pub async fn route_component_interaction(
    handler: &Handler,
    context: &Context,
    component: &ComponentInteraction,
) -> Result<()> {
    let custom_id = component.data.custom_id.as_str();

    match custom_id {
        // Setup buttons
        "setup_private" | "setup_public" => {
            commands::setup::handle_button(handler, context, component).await
        }

        // Help pagination buttons
        id if id.starts_with("help_page_") => {
            commands::help::handle_button(handler, context, component).await
        }

        // Add sticker buttons
        id if id.starts_with("add_sticker_") => {
            commands::add_sticker::handle_button(handler, context, component).await
        }

        // Stickers management buttons
        id if id.starts_with("remove_sticker_")
            || id == "clear_all_stickers"
            || id.starts_with("clear_category_stickers_")
            || id.starts_with("stickers_page_") =>
        {
            commands::stickers::handle_button(handler, context, component).await
        }

        // Unknown component interaction - ignore silently
        _ => {
            tracing::debug!("Unhandled component interaction: {}", custom_id);
            Ok(())
        }
    }
}
