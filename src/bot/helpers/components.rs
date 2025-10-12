use serenity::all::{
    ButtonStyle, ComponentInteraction, Context, CreateActionRow, CreateButton,
    CreateInteractionResponse, CreateInteractionResponseMessage,
};

/// Helper for building buttons more easily
#[allow(dead_code)]
pub struct ButtonBuilder {
    buttons: Vec<CreateButton>,
}

#[allow(dead_code)]
impl ButtonBuilder {
    pub fn new() -> Self {
        Self {
            buttons: Vec::new(),
        }
    }

    /// Add a primary button
    pub fn primary(mut self, custom_id: impl Into<String>, label: impl Into<String>) -> Self {
        self.buttons.push(
            CreateButton::new(custom_id.into())
                .label(label.into())
                .style(ButtonStyle::Primary),
        );
        self
    }

    /// Add a secondary button
    pub fn secondary(mut self, custom_id: impl Into<String>, label: impl Into<String>) -> Self {
        self.buttons.push(
            CreateButton::new(custom_id.into())
                .label(label.into())
                .style(ButtonStyle::Secondary),
        );
        self
    }

    /// Add a success button (green)
    pub fn success(mut self, custom_id: impl Into<String>, label: impl Into<String>) -> Self {
        self.buttons.push(
            CreateButton::new(custom_id.into())
                .label(label.into())
                .style(ButtonStyle::Success),
        );
        self
    }

    /// Add a danger button (red)
    pub fn danger(mut self, custom_id: impl Into<String>, label: impl Into<String>) -> Self {
        self.buttons.push(
            CreateButton::new(custom_id.into())
                .label(label.into())
                .style(ButtonStyle::Danger),
        );
        self
    }

    /// Add a custom styled button
    pub fn add_button(mut self, button: CreateButton) -> Self {
        self.buttons.push(button);
        self
    }

    /// Build the action row
    pub fn build(self) -> CreateActionRow {
        CreateActionRow::Buttons(self.buttons)
    }

    /// Build if there are buttons, otherwise return None
    pub fn build_optional(self) -> Option<CreateActionRow> {
        if self.buttons.is_empty() {
            None
        } else {
            Some(CreateActionRow::Buttons(self.buttons))
        }
    }
}

impl Default for ButtonBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper for responding to component interactions
#[allow(dead_code)]
pub struct ComponentResponseBuilder;

#[allow(dead_code)]
impl ComponentResponseBuilder {
    /// Send an ephemeral error message in response to a component interaction
    pub async fn error(
        context: &Context,
        interaction: &ComponentInteraction,
        message: impl Into<String>,
    ) -> anyhow::Result<()> {
        let response = CreateInteractionResponseMessage::new()
            .content(format!("[ERROR] {}", message.into()))
            .ephemeral(true);

        interaction
            .create_response(context, CreateInteractionResponse::Message(response))
            .await?;

        Ok(())
    }

    /// Send an ephemeral success message in response to a component interaction
    pub async fn success(
        context: &Context,
        interaction: &ComponentInteraction,
        message: impl Into<String>,
    ) -> anyhow::Result<()> {
        let response = CreateInteractionResponseMessage::new()
            .content(format!("[OK] {}", message.into()))
            .ephemeral(true);

        interaction
            .create_response(context, CreateInteractionResponse::Message(response))
            .await?;

        Ok(())
    }

    /// Update the message with new content
    pub async fn update_message(
        context: &Context,
        interaction: &ComponentInteraction,
        response: CreateInteractionResponseMessage,
    ) -> anyhow::Result<()> {
        interaction
            .create_response(context, CreateInteractionResponse::UpdateMessage(response))
            .await?;

        Ok(())
    }
}

/// Check if a custom_id matches a pattern
#[allow(dead_code)]
pub fn custom_id_matches(custom_id: &str, pattern: &str) -> bool {
    custom_id.starts_with(pattern)
}

/// Extract a value from a custom_id with a prefix
///
/// # Example
/// ```
/// let value = extract_custom_id_value("remove_sticker_123", "remove_sticker_");
/// assert_eq!(value, Some("123"));
/// ```
#[allow(dead_code)]
pub fn extract_custom_id_value<'a>(custom_id: &'a str, prefix: &str) -> Option<&'a str> {
    custom_id.strip_prefix(prefix)
}
