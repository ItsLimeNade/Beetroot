use serenity::all::{ButtonStyle, CreateActionRow, CreateButton};

/// Create pagination buttons for multi-page interfaces
///
/// # Arguments
/// * `prefix` - The prefix for button custom IDs (e.g., "help_page_")
/// * `current_page` - The current page number (1-indexed)
/// * `total_pages` - Total number of pages
///
/// # Returns
/// Optional CreateActionRow with Previous/Next buttons, or None if only 1 page
pub fn create_pagination_buttons(
    prefix: &str,
    current_page: u8,
    total_pages: u8,
) -> Option<CreateActionRow> {
    if total_pages <= 1 {
        return None;
    }

    let mut buttons = Vec::new();

    if current_page > 1 {
        buttons.push(
            CreateButton::new(format!("{}{}", prefix, current_page - 1))
                .label("◀ Previous")
                .style(ButtonStyle::Secondary),
        );
    }

    if current_page < total_pages {
        buttons.push(
            CreateButton::new(format!("{}{}", prefix, current_page + 1))
                .label("Next ▶")
                .style(ButtonStyle::Secondary),
        );
    }

    if buttons.is_empty() {
        None
    } else {
        Some(CreateActionRow::Buttons(buttons))
    }
}

/// Extract page number from a custom_id with a given prefix
///
/// # Arguments
/// * `custom_id` - The button custom ID (e.g., "help_page_2")
/// * `prefix` - The prefix to strip (e.g., "help_page_")
///
/// # Returns
/// The page number, or None if parsing fails
pub fn extract_page_number(custom_id: &str, prefix: &str) -> Option<u8> {
    custom_id.strip_prefix(prefix)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pagination_buttons_single_page() {
        let result = create_pagination_buttons("test_", 1, 1);
        assert!(result.is_none());
    }

    #[test]
    fn test_pagination_buttons_first_page() {
        let result = create_pagination_buttons("test_", 1, 3);
        assert!(result.is_some());
    }

    #[test]
    fn test_extract_page_number() {
        assert_eq!(extract_page_number("help_page_2", "help_page_"), Some(2));
        assert_eq!(extract_page_number("help_page_10", "help_page_"), Some(10));
        assert_eq!(extract_page_number("invalid", "help_page_"), None);
    }
}
