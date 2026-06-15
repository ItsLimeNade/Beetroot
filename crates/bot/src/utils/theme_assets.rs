use anyhow::{Result, anyhow};
use beetroot_core::Database;
use bonbon::theme::Theme;
use image::Rgba;

/// The 14 bonbon `Theme` color fields: `(json_key, human_label)`.
///
/// The JSON keys must match `bonbon::theme::Theme`'s field names exactly so the
/// stored blob round-trips through serde. The labels are used in command
/// choices and embeds.
pub const THEME_FIELDS: &[(&str, &str)] = &[
    ("background", "Background"),
    ("grid_major", "Major Grid"),
    ("grid_minor", "Minor Grid"),
    ("axis_lines", "Axis Lines"),
    ("text_primary", "Primary Text"),
    ("text_secondary", "Secondary Text"),
    ("text_dim", "Dim Text"),
    ("glucose_high", "High Glucose"),
    ("glucose_low", "Low Glucose"),
    ("glucose_in_range", "In-Range Glucose"),
    ("insulin", "Insulin"),
    ("carbs", "Carbs"),
    ("glucose_reading_fill", "Reading Fill"),
    ("glucose_reading_outline", "Reading Outline"),
];

/// Every color of a theme as `(json_key, color)`, in display order.
pub fn theme_colors(theme: &Theme) -> [(&'static str, Rgba<u8>); 14] {
    [
        ("background", theme.background),
        ("grid_major", theme.grid_major),
        ("grid_minor", theme.grid_minor),
        ("axis_lines", theme.axis_lines),
        ("text_primary", theme.text_primary),
        ("text_secondary", theme.text_secondary),
        ("text_dim", theme.text_dim),
        ("glucose_high", theme.glucose_high),
        ("glucose_low", theme.glucose_low),
        ("glucose_in_range", theme.glucose_in_range),
        ("insulin", theme.insulin),
        ("carbs", theme.carbs),
        ("glucose_reading_fill", theme.glucose_reading_fill),
        ("glucose_reading_outline", theme.glucose_reading_outline),
    ]
}

/// Human label for a field key, falling back to the key itself.
pub fn field_label(key: &str) -> &str {
    THEME_FIELDS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, label)| *label)
        .unwrap_or(key)
}

/// `#RRGGBBAA` for a color.
pub fn rgba_to_hex(c: Rgba<u8>) -> String {
    format!("#{:02x}{:02x}{:02x}{:02x}", c.0[0], c.0[1], c.0[2], c.0[3])
}

/// Validate a user-supplied hex color and normalize to lowercase `#......`.
/// Accepts 6 (RGB) or 8 (RGBA) hex digits, with or without a leading `#`.
pub fn normalize_hex(input: &str) -> Result<String> {
    let h = input.trim().trim_start_matches('#').to_lowercase();
    let ok = (h.len() == 6 || h.len() == 8) && h.chars().all(|c| c.is_ascii_hexdigit());
    if !ok {
        return Err(anyhow!(
            "`{}` is not a valid hex color. Use `#RRGGBB` or `#RRGGBBAA` (e.g. `#ff5555`).",
            input
        ));
    }
    Ok(format!("#{h}"))
}

/// Serialize a theme into the canonical storage JSON (all 14 fields as hex).
pub fn theme_to_json(theme: &Theme) -> String {
    let map: serde_json::Map<String, serde_json::Value> = theme_colors(theme)
        .iter()
        .map(|(k, c)| (k.to_string(), serde_json::Value::String(rgba_to_hex(*c))))
        .collect();
    serde_json::Value::Object(map).to_string()
}

/// Parse stored JSON into a bonbon `Theme`.
pub fn json_to_theme(data: &str) -> Result<Theme> {
    serde_json::from_str::<Theme>(data).map_err(|e| {
        anyhow!(
            "Theme data is not valid. Each of the 14 fields must be a hex color string. ({e})"
        )
    })
}

/// Override a single color field inside stored JSON, returning the new JSON.
pub fn set_field(data: &str, field: &str, hex: &str) -> Result<String> {
    if !THEME_FIELDS.iter().any(|(k, _)| *k == field) {
        return Err(anyhow!("Unknown theme field `{field}`."));
    }
    let normalized = normalize_hex(hex)?;
    let mut map: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(data).map_err(|e| anyhow!("Stored theme data is corrupt: {e}"))?;
    map.insert(field.to_string(), serde_json::Value::String(normalized));
    Ok(serde_json::Value::Object(map).to_string())
}

/// Look up a builtin theme by its bonbon name (e.g. `"beetroot_dark"`).
pub fn builtin_by_name(name: &str) -> Option<Theme> {
    Theme::builtins()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
}

/// Names of all builtin themes, in display order.
pub fn builtin_names() -> Vec<&'static str> {
    Theme::builtins().into_iter().map(|(n, _)| n).collect()
}

/// Resolve a user's `active_theme` selector into a concrete `Theme`.
///
/// `None` or any unresolvable selector falls back to the default dark theme.
/// `"custom:<name>"` loads the user's stored theme; a builtin name resolves via
/// [`builtin_by_name`].
pub async fn resolve_user_theme(db: &Database, discord_id: u64, active: Option<&str>) -> Theme {
    let Some(selector) = active else {
        return Theme::dark();
    };

    if let Some(name) = selector.strip_prefix("custom:") {
        match db.get_theme_by_name(discord_id, name).await {
            Ok(Some(row)) => match json_to_theme(&row.data) {
                Ok(theme) => return theme,
                Err(e) => tracing::warn!("[THEME] custom '{}' invalid, using dark: {}", name, e),
            },
            Ok(None) => tracing::warn!("[THEME] active custom '{}' no longer exists", name),
            Err(e) => tracing::warn!("[THEME] failed to load custom '{}': {}", name, e),
        }
        return Theme::dark();
    }

    builtin_by_name(selector).unwrap_or_else(Theme::dark)
}
