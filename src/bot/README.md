# Bot Module Structure

This directory contains all Discord bot-related code, organized for modularity and maintainability.

## Directory Structure

```
src/bot/
├── mod.rs                    # Module exports
├── handler.rs                # Handler struct definition
├── event_handler.rs          # EventHandler implementation
├── init.rs                   # Bot initialization
├── command_registry.rs       # Command registration
├── component_router.rs       # Component interaction routing
├── version_checker.rs        # Version update notifications
└── helpers/                  # Helper utilities
    ├── mod.rs                # Helper exports
    ├── command_handler.rs    # Command routing logic
    ├── components.rs         # Button/component helpers
    └── pagination.rs         # Pagination utilities
```

## Module Descriptions

### Core Modules

#### `handler.rs`
Defines the `Handler` struct that holds bot state:
- Nightscout client
- Database connection
- Font for graph rendering

#### `event_handler.rs`
Implements `EventHandler` trait for Serenity:
- Routes all interactions (commands & components)
- Handles errors gracefully
- Registers commands on bot ready

#### `init.rs`
Bot initialization and startup:
- Loads environment variables
- Creates Handler instance
- Starts Discord client

#### `command_registry.rs`
Central location for command registration:
- Returns all slash commands
- Returns all context menu commands
- Easy to add new commands

#### `component_router.rs`
Routes button/component interactions:
- Pattern-based routing
- Forwards to appropriate command handlers
- Handles unknown interactions gracefully

#### `version_checker.rs`
Manages version update notifications:
- Checks user's last seen version
- Sends update notifications
- Updates database with new version

### Helper Modules

#### `helpers/command_handler.rs`
Command routing and validation:
- `handle_slash_command()` - Routes slash commands
- `handle_context_command()` - Routes context menu commands
- Checks user setup requirements
- Consistent error handling

#### `helpers/components.rs`
Button and component utilities:
- `ButtonBuilder` - Fluent API for creating buttons
- `ComponentResponseBuilder` - Helper for responding to interactions
- `custom_id_matches()` - Pattern matching helper
- `extract_custom_id_value()` - Extract data from custom IDs

#### `helpers/pagination.rs`
Pagination utilities for multi-page interfaces:
- `create_pagination_buttons()` - Generate prev/next buttons
- `extract_page_number()` - Parse page from custom_id

## Usage Examples

### Adding a New Command

1. Create command file in `src/commands/your_command.rs`
2. Add to `src/commands/mod.rs`
3. Add registration to `command_registry.rs`:
```rust
commands::your_command::register(),
```
4. Add routing in `helpers/command_handler.rs`:
```rust
"your-command" => commands::your_command::run(handler, context, command).await,
```

### Using Pagination Helper

```rust
use crate::bot::helpers::pagination;

fn create_page(page: u8, total: u8) -> (CreateEmbed, Option<CreateActionRow>) {
    let embed = CreateEmbed::new()
        .title(format!("Page {}/{}", page, total));

    let buttons = pagination::create_pagination_buttons("prefix_", page, total);

    (embed, buttons)
}

// In button handler:
if let Some(page) = pagination::extract_page_number(custom_id, "prefix_") {
    // Handle page change
}
```

### Using Button Builder

```rust
use crate::bot::helpers::ButtonBuilder;

let buttons = ButtonBuilder::new()
    .success("confirm_action", "Confirm")
    .danger("cancel_action", "Cancel")
    .build();
```

### Adding Component Interactions

Add pattern to `component_router.rs`:
```rust
id if id.starts_with("your_prefix_") => {
    commands::your_command::handle_button(handler, context, component).await
}
```

## Benefits of This Structure

1. **Separation of Concerns**: Bot logic separated from business logic
2. **Modularity**: Easy to find and modify specific functionality
3. **Reusability**: Helpers can be used across multiple commands
4. **Maintainability**: Clear structure makes it easy to understand
5. **Testability**: Isolated modules are easier to test
6. **Scalability**: Easy to add new commands and features

## Main Function

The main function in `src/main.rs` is now minimal:
```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(...)
        .init();

    // Start the bot
    bot::init::start_bot().await
}
```

All bot-specific logic is contained within the `bot` module.
