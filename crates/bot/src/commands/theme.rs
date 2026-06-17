use crate::data::{Context, Error};
use crate::utils::emojis;
use crate::utils::theme_assets;
use bonbon::theme::Theme;
use image::{ImageEncoder, Rgba, RgbaImage};
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateAttachment, CreateEmbed, CreateEmbedFooter};

/// Maximum number of custom themes a single user may keep.
const MAX_THEMES_PER_USER: i64 = 15;
/// Maximum accepted size of an imported theme file.
const MAX_IMPORT_BYTES: u32 = 64 * 1024;

/// Create, edit and apply color themes for your glucose visuals.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel",
    subcommands("list", "set", "create", "edit", "delete", "import", "view")
)]
pub async fn theme(_ctx: Context<'_>) -> Result<(), Error> {
    // Parent of a slash command group; never invoked directly.
    Ok(())
}

/// List the builtin themes and your custom themes.
#[poise::command(slash_command)]
#[track_analytics("theme_list")]
pub async fn list(ctx: Context<'_>) -> Result<(), Error> {
    let db = &ctx.data().database;
    let user_id = ctx.author().id.get();

    let active = db.get_user(user_id).await?.and_then(|u| u.active_theme);
    let active = active.as_deref();

    let mark = |selector: &str| {
        if active == Some(selector) {
            format!(" {} **(active)**", emojis::celebration())
        } else {
            String::new()
        }
    };

    let builtins = theme_assets::builtin_names()
        .into_iter()
        .map(|name| format!("- `{}`{}", name, mark(name)))
        .collect::<Vec<_>>()
        .join("\n");

    let custom_rows = db.get_user_themes(user_id).await?;
    let custom = if custom_rows.is_empty() {
        "_None yet. Use `/theme create` to make one._".to_string()
    } else {
        custom_rows
            .iter()
            .map(|t| format!("- `{}`{}", t.name, mark(&format!("custom:{}", t.name))))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let active_display = match active {
        None => "Default (`beetroot_dark`)".to_string(),
        Some(sel) => match sel.strip_prefix("custom:") {
            Some(name) => format!("`{}` (custom)", name),
            None => format!("`{}`", sel),
        },
    };

    let embed = CreateEmbed::new()
        .title(format!("{} Themes", emojis::image_mode()))
        .color(Colour::from_rgb(87, 189, 79))
        .field(format!("{} Active", emojis::celebration()), active_display, false)
        .field(format!("{} Builtin", emojis::image_mode()), builtins, false)
        .field(
            format!(
                "{} Your themes ({}/{})",
                emojis::image_mode(),
                custom_rows.len(),
                MAX_THEMES_PER_USER
            ),
            custom,
            false,
        )
        .footer(CreateEmbedFooter::new(
            "Apply one with /theme set • preview with /theme view",
        ));

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}

/// Apply a theme (builtin name or one of your custom themes) to your visuals.
#[poise::command(slash_command)]
#[track_analytics("theme_set")]
pub async fn set(
    ctx: Context<'_>,
    #[description = "Theme name (a builtin or one of your custom themes)"] name: String,
) -> Result<(), Error> {
    let db = &ctx.data().database;
    let user_id = ctx.author().id.get();
    db.ensure_user_row(user_id).await?;

    let name = name.trim();

    // Prefer the user's own theme so custom names always win.
    let selector = if db.get_theme_by_name(user_id, name).await?.is_some() {
        format!("custom:{}", name)
    } else if theme_assets::builtin_by_name(name).is_some() {
        name.to_string()
    } else {
        send_error!(
            ctx,
            "No Such Theme",
            format!(
                "`{}` isn't a builtin or one of your themes. Run `/theme list` to see what's available.",
                name
            )
        );
        return Ok(());
    };

    db.set_active_theme(user_id, Some(&selector)).await?;
    tracing::info!(
        user = %crate::logging::redact(user_id),
        theme = %selector,
        "active theme set"
    );

    let embed = CreateEmbed::new()
        .title(format!("{} Theme Applied", emojis::celebration()))
        .description(format!(
            "**{}** is now your active theme. It'll show up on your next `/graph`, `/tir` and image-mode `/bg`.",
            name
        ))
        .color(Colour::DARK_GREEN);

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}

/// Create a custom theme as a copy of a builtin you can then edit.
#[poise::command(slash_command)]
#[track_analytics("theme_create")]
pub async fn create(
    ctx: Context<'_>,
    #[description = "A name for your theme"] name: String,
    #[description = "Builtin to start from"] base: ThemeBaseChoice,
) -> Result<(), Error> {
    let db = &ctx.data().database;
    let user_id = ctx.author().id.get();
    db.ensure_user_row(user_id).await?;

    let name = name.trim();
    if let Err(msg) = validate_theme_name(name) {
        send_error!(ctx, "Invalid Name", msg);
        return Ok(());
    }

    if theme_assets::builtin_by_name(name).is_some() {
        send_error!(
            ctx,
            "Reserved Name",
            "That name belongs to a builtin theme. Pick a different name."
        );
        return Ok(());
    }

    let count = db.count_user_themes(user_id).await?;
    if count >= MAX_THEMES_PER_USER {
        send_error!(
            ctx,
            "Too Many Themes",
            format!(
                "You already have {}/{} themes. Delete one with `/theme delete` first.",
                count, MAX_THEMES_PER_USER
            )
        );
        return Ok(());
    }

    let data = theme_assets::theme_to_json(&base.theme());

    if !db.insert_theme(user_id, name, &data).await? {
        send_error!(
            ctx,
            "Name Taken",
            format!("You already have a theme called `{}`.", name)
        );
        return Ok(());
    }
    tracing::info!(
        user = %crate::logging::redact(user_id),
        theme = %name,
        base = base.builtin_name(),
        "custom theme created"
    );

    let embed = CreateEmbed::new()
        .title(format!("{} Theme Created", emojis::celebration()))
        .description(format!(
            "**{}** was created from `{}`.\n\nTweak colors with `/theme edit name:{} field:… color:#rrggbb`, preview with `/theme view`, then apply with `/theme set`.",
            name,
            base.builtin_name(),
            name
        ))
        .color(Colour::DARK_GREEN);

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}

/// Change one color of a custom theme.
#[poise::command(slash_command)]
#[track_analytics("theme_edit")]
pub async fn edit(
    ctx: Context<'_>,
    #[description = "Which of your themes to edit"] name: String,
    #[description = "Which color to change"] field: ThemeFieldChoice,
    #[description = "New color as hex, e.g. #ff5555 or #ff5555cc"] color: String,
) -> Result<(), Error> {
    let db = &ctx.data().database;
    let user_id = ctx.author().id.get();

    let name = name.trim();
    let Some(row) = db.get_theme_by_name(user_id, name).await? else {
        send_error!(
            ctx,
            "No Such Theme",
            format!("You don't have a theme called `{}`.", name)
        );
        return Ok(());
    };

    let updated = match theme_assets::set_field(&row.data, field.key(), &color) {
        Ok(json) => json,
        Err(e) => {
            send_error!(ctx, "Invalid Color", e.to_string());
            return Ok(());
        }
    };

    db.update_theme_data(user_id, name, &updated).await?;
    tracing::info!(
        user = %crate::logging::redact(user_id),
        theme = %name,
        field = field.key(),
        "theme field updated"
    );

    let embed = CreateEmbed::new()
        .title(format!("{} Theme Updated", emojis::image_mode()))
        .description(format!(
            "Set **{}** to `{}` on `{}`.\n\nPreview it with `/theme view name:{}`.",
            theme_assets::field_label(field.key()),
            theme_assets::normalize_hex(&color).unwrap_or(color),
            name,
            name
        ))
        .color(Colour::DARK_GREEN);

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}

/// Delete one of your custom themes.
#[poise::command(slash_command)]
#[track_analytics("theme_delete")]
pub async fn delete(
    ctx: Context<'_>,
    #[description = "Which of your themes to delete"] name: String,
) -> Result<(), Error> {
    let db = &ctx.data().database;
    let user_id = ctx.author().id.get();

    let name = name.trim();
    if !db.delete_theme(user_id, name).await? {
        send_error!(
            ctx,
            "No Such Theme",
            format!("You don't have a theme called `{}`.", name)
        );
        return Ok(());
    }

    // If the deleted theme was active, fall back to the default.
    let active = db.get_user(user_id).await?.and_then(|u| u.active_theme);
    let was_active = active.as_deref() == Some(&format!("custom:{}", name));
    if was_active {
        db.set_active_theme(user_id, None).await?;
    }
    tracing::info!(
        user = %crate::logging::redact(user_id),
        theme = %name,
        reset_active = was_active,
        "custom theme deleted"
    );

    let embed = CreateEmbed::new()
        .title(format!("{} Theme Deleted", emojis::remove_user()))
        .description(format!("Removed your theme `{}`.", name))
        .color(Colour::DARK_RED);

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}

/// Import a theme from a bonbon JSON file.
#[poise::command(slash_command)]
#[track_analytics("theme_import")]
pub async fn import(
    ctx: Context<'_>,
    #[description = "A name for the imported theme"] name: String,
    #[description = "A bonbon theme .json file"] file: serenity::Attachment,
) -> Result<(), Error> {
    let db = &ctx.data().database;
    let user_id = ctx.author().id.get();
    db.ensure_user_row(user_id).await?;

    let name = name.trim();
    if let Err(msg) = validate_theme_name(name) {
        send_error!(ctx, "Invalid Name", msg);
        return Ok(());
    }
    if theme_assets::builtin_by_name(name).is_some() {
        send_error!(
            ctx,
            "Reserved Name",
            "That name belongs to a builtin theme. Pick a different name."
        );
        return Ok(());
    }

    if file.size > MAX_IMPORT_BYTES {
        send_error!(
            ctx,
            "File Too Large",
            "Theme files should be tiny JSON (under 64 KB)."
        );
        return Ok(());
    }

    let count = db.count_user_themes(user_id).await?;
    if count >= MAX_THEMES_PER_USER {
        send_error!(
            ctx,
            "Too Many Themes",
            format!(
                "You already have {}/{} themes. Delete one with `/theme delete` first.",
                count, MAX_THEMES_PER_USER
            )
        );
        return Ok(());
    }

    crate::tips::safe_defer_ephemeral(ctx).await?;

    let bytes = match file.download().await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, "theme file download failed");
            send_error!(
                ctx,
                "Download Failed",
                "Couldn't read the uploaded file. Please try again."
            );
            return Ok(());
        }
    };

    let text = match String::from_utf8(bytes) {
        Ok(t) => t,
        Err(_) => {
            send_error!(ctx, "Invalid File", "That file isn't valid UTF-8 text.");
            return Ok(());
        }
    };

    // Validate and normalize through bonbon's Theme so storage is always clean.
    let parsed = match theme_assets::json_to_theme(&text) {
        Ok(t) => t,
        Err(e) => {
            send_error!(ctx, "Invalid Theme", e.to_string());
            return Ok(());
        }
    };
    let data = theme_assets::theme_to_json(&parsed);

    if !db.insert_theme(user_id, name, &data).await? {
        send_error!(
            ctx,
            "Name Taken",
            format!("You already have a theme called `{}`.", name)
        );
        return Ok(());
    }
    tracing::info!(
        user = %crate::logging::redact(user_id),
        theme = %name,
        "theme imported from file"
    );

    let embed = CreateEmbed::new()
        .title(format!("{} Theme Imported", emojis::celebration()))
        .description(format!(
            "Imported **{}**. Preview it with `/theme view name:{}` and apply with `/theme set`.",
            name, name
        ))
        .color(Colour::DARK_GREEN);

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}

/// Render a color-swatch preview of a theme.
#[poise::command(slash_command)]
#[track_analytics("theme_view")]
pub async fn view(
    ctx: Context<'_>,
    #[description = "Theme name (builtin or your custom theme)"] name: String,
) -> Result<(), Error> {
    let db = &ctx.data().database;
    let user_id = ctx.author().id.get();

    let name = name.trim();

    let theme = if let Some(row) = db.get_theme_by_name(user_id, name).await? {
        match theme_assets::json_to_theme(&row.data) {
            Ok(t) => t,
            Err(e) => {
                send_error!(ctx, "Corrupt Theme", e.to_string());
                return Ok(());
            }
        }
    } else if let Some(t) = theme_assets::builtin_by_name(name) {
        t
    } else {
        send_error!(
            ctx,
            "No Such Theme",
            format!("`{}` isn't a builtin or one of your themes.", name)
        );
        return Ok(());
    };

    let legend = theme_assets::theme_colors(&theme)
        .iter()
        .map(|(key, color)| {
            format!(
                "`{}` {}",
                theme_assets::rgba_to_hex(*color),
                theme_assets::field_label(key)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let theme_for_image = theme.clone();
    let img_buffer = tokio::task::spawn_blocking(move || render_swatch(&theme_for_image)).await??;

    let attachment = CreateAttachment::bytes(img_buffer, "theme.png");

    let embed = CreateEmbed::new()
        .title(format!("{} Theme: {}", emojis::image_mode(), name))
        .description(legend)
        .color(rgba_to_colour(theme.glucose_in_range))
        .image("attachment://theme.png");

    ctx.send(
        poise::CreateReply::default()
            .embed(embed)
            .attachment(attachment)
            .ephemeral(true),
    )
    .await?;

    Ok(())
}

/// Draw a 7×2 grid of color swatches over the theme background.
fn render_swatch(theme: &Theme) -> Result<Vec<u8>, Error> {
    const COLS: u32 = 7;
    const ROWS: u32 = 2;
    const CELL: u32 = 96;
    const PAD: u32 = 16;
    const SWATCH: u32 = CELL - PAD;

    let width = COLS * CELL + PAD;
    let height = ROWS * CELL + PAD;

    let mut img = RgbaImage::from_pixel(width, height, opaque(theme.background));

    for (i, (_, color)) in theme_assets::theme_colors(theme).iter().enumerate() {
        let col = i as u32 % COLS;
        let row = i as u32 / COLS;
        let x0 = PAD + col * CELL;
        let y0 = PAD + row * CELL;

        // subtle outline using the theme's outline color
        fill_rect(
            &mut img,
            x0.saturating_sub(2),
            y0.saturating_sub(2),
            SWATCH + 4,
            SWATCH + 4,
            opaque(theme.glucose_reading_outline),
        );
        // the swatch itself, alpha composited over the background
        fill_rect_blended(&mut img, x0, y0, SWATCH, SWATCH, *color);
    }

    let mut buffer = Vec::with_capacity(40_000);
    image::codecs::png::PngEncoder::new_with_quality(
        &mut buffer,
        image::codecs::png::CompressionType::Level(6),
        image::codecs::png::FilterType::NoFilter,
    )
    .write_image(
        img.as_raw(),
        img.width(),
        img.height(),
        image::ExtendedColorType::Rgba8,
    )?;
    Ok(buffer)
}

fn opaque(c: Rgba<u8>) -> Rgba<u8> {
    Rgba([c.0[0], c.0[1], c.0[2], 255])
}

fn fill_rect(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, color: Rgba<u8>) {
    let (iw, ih) = (img.width(), img.height());
    for yy in y..(y + h).min(ih) {
        for xx in x..(x + w).min(iw) {
            img.put_pixel(xx, yy, color);
        }
    }
}

/// Alpha composite `color` over whatever is already in the rect.
fn fill_rect_blended(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, color: Rgba<u8>) {
    let (iw, ih) = (img.width(), img.height());
    let a = color.0[3] as f32 / 255.0;
    let inv = 1.0 - a;
    for yy in y..(y + h).min(ih) {
        for xx in x..(x + w).min(iw) {
            let bg = img.get_pixel(xx, yy).0;
            let blended = Rgba([
                (color.0[0] as f32 * a + bg[0] as f32 * inv) as u8,
                (color.0[1] as f32 * a + bg[1] as f32 * inv) as u8,
                (color.0[2] as f32 * a + bg[2] as f32 * inv) as u8,
                255,
            ]);
            img.put_pixel(xx, yy, blended);
        }
    }
}

fn rgba_to_colour(c: Rgba<u8>) -> Colour {
    Colour::from_rgb(c.0[0], c.0[1], c.0[2])
}

fn validate_theme_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Give your theme a name.".to_string());
    }
    if name.chars().count() > 32 {
        return Err("Theme names must be 32 characters or fewer.".to_string());
    }
    if name.starts_with("custom:") {
        return Err("Theme names can't start with `custom:`.".to_string());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '-' || c == '_')
    {
        return Err("Use only letters, numbers, spaces, hyphens and underscores.".to_string());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, poise::ChoiceParameter)]
pub enum ThemeBaseChoice {
    #[name = "Beetroot Dark"]
    BeetrootDark,
    #[name = "Beetroot Light"]
    BeetrootLight,
    #[name = "Licorice Dark"]
    LicoriceDark,
    #[name = "Watermelon Dark"]
    WatermelonDark,
    #[name = "Paper Light"]
    PaperLight,
    #[name = "Ube Light"]
    UbeLight,
}

impl ThemeBaseChoice {
    fn theme(self) -> Theme {
        match self {
            Self::BeetrootDark => Theme::dark(),
            Self::BeetrootLight => Theme::light(),
            Self::LicoriceDark => Theme::licorice_dark(),
            Self::WatermelonDark => Theme::watermelon_dark(),
            Self::PaperLight => Theme::paper_light(),
            Self::UbeLight => Theme::ube_light(),
        }
    }

    fn builtin_name(self) -> &'static str {
        match self {
            Self::BeetrootDark => "beetroot_dark",
            Self::BeetrootLight => "beetroot_light",
            Self::LicoriceDark => "licorice_dark",
            Self::WatermelonDark => "watermelon_dark",
            Self::PaperLight => "paper_light",
            Self::UbeLight => "ube_light",
        }
    }
}

#[derive(Debug, Clone, Copy, poise::ChoiceParameter)]
pub enum ThemeFieldChoice {
    #[name = "Background"]
    Background,
    #[name = "Major Grid"]
    GridMajor,
    #[name = "Minor Grid"]
    GridMinor,
    #[name = "Axis Lines"]
    AxisLines,
    #[name = "Primary Text"]
    TextPrimary,
    #[name = "Secondary Text"]
    TextSecondary,
    #[name = "Dim Text"]
    TextDim,
    #[name = "High Glucose"]
    GlucoseHigh,
    #[name = "Low Glucose"]
    GlucoseLow,
    #[name = "In-Range Glucose"]
    GlucoseInRange,
    #[name = "Insulin"]
    Insulin,
    #[name = "Carbs"]
    Carbs,
    #[name = "Reading Fill"]
    GlucoseReadingFill,
    #[name = "Reading Outline"]
    GlucoseReadingOutline,
}

impl ThemeFieldChoice {
    fn key(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::GridMajor => "grid_major",
            Self::GridMinor => "grid_minor",
            Self::AxisLines => "axis_lines",
            Self::TextPrimary => "text_primary",
            Self::TextSecondary => "text_secondary",
            Self::TextDim => "text_dim",
            Self::GlucoseHigh => "glucose_high",
            Self::GlucoseLow => "glucose_low",
            Self::GlucoseInRange => "glucose_in_range",
            Self::Insulin => "insulin",
            Self::Carbs => "carbs",
            Self::GlucoseReadingFill => "glucose_reading_fill",
            Self::GlucoseReadingOutline => "glucose_reading_outline",
        }
    }
}
