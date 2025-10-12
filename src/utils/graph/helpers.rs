use anyhow::{Result, anyhow};
use image::RgbaImage;

/// Download a sticker image from a URL
pub async fn download_sticker_image(url: &str) -> Result<image::DynamicImage> {
    tracing::debug!("[STICKER] Downloading sticker from: {}", url);

    let response = reqwest::get(url).await?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "Failed to download sticker: HTTP {}",
            response.status()
        ));
    }

    let bytes = response.bytes().await?;
    let img = image::load_from_memory(&bytes)?;

    tracing::debug!(
        "[STICKER] Successfully downloaded sticker ({} bytes)",
        bytes.len()
    );
    Ok(img)
}

/// Draw a dashed vertical line on the image
pub fn draw_dashed_vertical_line(
    img: &mut RgbaImage,
    x: f32,
    y_start: f32,
    y_end: f32,
    color: image::Rgba<u8>,
    dash_length: i32,
    gap_length: i32,
) {
    let x = x.round() as i32;
    let y_start = y_start.round() as i32;
    let y_end = y_end.round() as i32;

    let mut y = y_start;
    let mut drawing_dash = true;

    while y < y_end {
        if drawing_dash {
            let dash_end = (y + dash_length).min(y_end);
            for py in y..dash_end {
                if x >= 0 && x < img.width() as i32 && py >= 0 && py < img.height() as i32 {
                    img.put_pixel(x as u32, py as u32, color);
                }
            }
            y += dash_length;
        } else {
            y += gap_length;
        }
        drawing_dash = !drawing_dash;
    }
}

/// Draw a dashed horizontal line on the image
pub fn draw_dashed_horizontal_line(
    img: &mut RgbaImage,
    y: f32,
    x_start: f32,
    x_end: f32,
    color: image::Rgba<u8>,
    dash_length: i32,
    gap_length: i32,
) {
    let y = y.round() as i32;
    let x_start = x_start.round() as i32;
    let x_end = x_end.round() as i32;

    let mut x = x_start;
    let mut drawing_dash = true;

    while x < x_end {
        if drawing_dash {
            let dash_end = (x + dash_length).min(x_end);
            for px in x..dash_end {
                if px >= 0 && px < img.width() as i32 && y >= 0 && y < img.height() as i32 {
                    img.put_pixel(px as u32, y as u32, color);
                }
            }
            x += dash_length;
        } else {
            x += gap_length;
        }
        drawing_dash = !drawing_dash;
    }
}
