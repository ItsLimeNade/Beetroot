use super::StickerPlacement;
use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use image::{DynamicImage, RgbaImage};
use std::collections::HashMap;

/// Parameters needed to reconstruct bonbon's coordinate system
/// so we can accurately position stickers.
#[derive(Debug, Clone)]
pub struct GraphCoordParams {
    pub width: u32,
    pub height: u32,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub y_min: f32,
    pub y_max: f32,
    pub margin_left: Option<f32>,
    pub margin_right: Option<f32>,
    pub margin_top: Option<f32>,
    pub margin_bottom: Option<f32>,
}

struct PlotViewport {
    plot_left: f32,
    #[allow(dead_code)]
    plot_right: f32,
    plot_top: f32,
    plot_bottom: f32,
    plot_w: f32,
    plot_h: f32,
}

impl GraphCoordParams {
    fn viewport(&self) -> PlotViewport {
        let s = self.width as f32 / 1200.0;

        let ml = self.margin_left.unwrap_or(120.0 * s);
        let mr = self.margin_right.unwrap_or(60.0 * s);
        let mt = self.margin_top.unwrap_or(80.0 * s);
        let mb = self.margin_bottom.unwrap_or(100.0 * s);

        let plot_left = ml;
        let plot_top = mt;
        let plot_right = self.width as f32 - mr;
        let plot_bottom = self.height as f32 - mb;

        PlotViewport {
            plot_left,
            plot_right,
            plot_top,
            plot_bottom,
            plot_w: plot_right - plot_left,
            plot_h: plot_bottom - plot_top,
        }
    }

    fn project_x(&self, vp: &PlotViewport, time: DateTime<Utc>) -> f32 {
        let time_span_secs = (self.end_time - self.start_time).num_seconds().max(1) as f32;
        let offset = (time - self.start_time).num_seconds() as f32;
        vp.plot_left + (offset / time_span_secs) * vp.plot_w
    }

    fn project_y(&self, vp: &PlotViewport, sgv: f32) -> f32 {
        let clamped = sgv.clamp(self.y_min, self.y_max);
        let ratio = (clamped - self.y_min) / (self.y_max - self.y_min);
        vp.plot_bottom - (ratio * vp.plot_h)
    }
}

/// Sticker max dimension (width or height) after resize.
const STICKER_MAX_SIZE: u32 = 270;
/// Minimum distance in pixels between sticker center and glucose curve.
const CURVE_CLEARANCE: f32 = 80.0;
/// Padding from the edge of the plot area so stickers don't clip.
const EDGE_PADDING: f32 = 10.0;
/// Alpha multiplier (0.0 = invisible, 1.0 = fully opaque).
const STICKER_ALPHA: f32 = 0.75;

/// Overlay stickers onto a bonbon-generated graph image.
///
/// 1. Downloads all unique sticker URLs in parallel (single HTTP call per URL).
/// 2. Resizes once per unique URL using a fast Triangle filter.
/// 3. Places each sticker AWAY from the glucose curve so readings stay readable.
pub async fn overlay_stickers_on_graph(
    graph: &mut RgbaImage,
    placements: &[StickerPlacement],
    entry_times: &[DateTime<Utc>],
    sgv_values: &[f32],
    params: &GraphCoordParams,
) -> Result<()> {
    if placements.is_empty() {
        return Ok(());
    }

    let vp = params.viewport();

    tracing::info!(
        "[STICKER] Overlaying {} stickers onto {}x{} graph",
        placements.len(),
        graph.width(),
        graph.height()
    );

    let unique_urls: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        placements
            .iter()
            .filter_map(|p| {
                if seen.insert(p.sticker.sticker_url.clone()) {
                    Some(p.sticker.sticker_url.clone())
                } else {
                    None
                }
            })
            .collect()
    };

    let download_start = std::time::Instant::now();

    let download_futures: Vec<_> = unique_urls
        .iter()
        .map(|url| download_and_resize(url))
        .collect();

    let download_results = futures::future::join_all(download_futures).await;

    let mut cache: HashMap<String, RgbaImage> = HashMap::new();
    for (url, result) in unique_urls.into_iter().zip(download_results) {
        match result {
            Ok(img) => {
                cache.insert(url, img);
            }
            Err(e) => {
                tracing::warn!("[STICKER] Failed to prepare '{}': {}", url, e);
            }
        }
    }

    tracing::debug!(
        "[STICKER] Downloaded & resized {} unique stickers in {:?}",
        cache.len(),
        download_start.elapsed()
    );

    for placement in placements {
        let idx = placement.entry_index;
        if idx >= entry_times.len() || idx >= sgv_values.len() {
            continue;
        }

        let resized = match cache.get(&placement.sticker.sticker_url) {
            Some(img) => img,
            None => continue, // download failed earlier
        };

        let x = params.project_x(&vp, entry_times[idx]);
        let curve_y = params.project_y(&vp, sgv_values[idx]);

        let (sticker_w, sticker_h) = resized.dimensions();
        let half_h = sticker_h as f32 / 2.0;

        let space_above = curve_y - vp.plot_top;
        let space_below = vp.plot_bottom - curve_y;

        let min_center_y = vp.plot_top + half_h + EDGE_PADDING;
        let max_center_y = vp.plot_bottom - half_h - EDGE_PADDING;

        let y = if space_above > space_below {
            (curve_y - CURVE_CLEARANCE - half_h).clamp(min_center_y, max_center_y)
        } else {
            (curve_y + CURVE_CLEARANCE + half_h).clamp(min_center_y, max_center_y)
        };

        // Also clamp X so the sticker doesn't overflow left/right
        let half_w = sticker_w as f32 / 2.0;
        let clamped_x = x.clamp(
            vp.plot_left + half_w + EDGE_PADDING,
            vp.plot_left + vp.plot_w - half_w - EDGE_PADDING,
        );

        let paste_x = (clamped_x - half_w) as i32;
        let paste_y = (y - half_h) as i32;

        composite_with_alpha(graph, resized, paste_x, paste_y, STICKER_ALPHA);

        tracing::debug!(
            "[STICKER] Drew '{}' at pixel ({}, {}) — curve_y={:.0}, placed {}",
            placement.sticker.display_name.as_deref().unwrap_or("?"),
            paste_x,
            paste_y,
            curve_y,
            if space_above > space_below {
                "above"
            } else {
                "below"
            },
        );
    }

    Ok(())
}

/// Download a sticker image and resize it. Called once per unique URL.
async fn download_and_resize(url: &str) -> Result<RgbaImage> {
    let sticker_img = download_sticker_image(url).await?;
    let sticker_rgba = sticker_img.to_rgba8();
    let (sw, sh) = sticker_rgba.dimensions();

    let scale = STICKER_MAX_SIZE as f32 / sw.max(sh) as f32;
    let new_w = ((sw as f32 * scale) as u32).max(1);
    let new_h = ((sh as f32 * scale) as u32).max(1);

    // Triangle filter is ~4x faster than Lanczos3, perfectly fine for stickers
    let resized = image::imageops::resize(
        &sticker_rgba,
        new_w,
        new_h,
        image::imageops::FilterType::Triangle,
    );

    Ok(resized)
}

/// Alpha-composite a sticker image onto the graph.
fn composite_with_alpha(
    base: &mut RgbaImage,
    overlay: &RgbaImage,
    offset_x: i32,
    offset_y: i32,
    alpha_mult: f32,
) {
    let (base_w, base_h) = base.dimensions();
    let (ov_w, ov_h) = overlay.dimensions();

    // Pre-compute clipped bounds so we skip the per-pixel bounds check
    let x_start = offset_x.max(0) as u32;
    let y_start = offset_y.max(0) as u32;
    let x_end = ((offset_x + ov_w as i32) as u32).min(base_w);
    let y_end = ((offset_y + ov_h as i32) as u32).min(base_h);

    for by in y_start..y_end {
        for bx in x_start..x_end {
            let ox = (bx as i32 - offset_x) as u32;
            let oy = (by as i32 - offset_y) as u32;

            let src = overlay.get_pixel(ox, oy);
            if src[3] == 0 {
                continue;
            }

            let src_alpha = (src[3] as f32 / 255.0) * alpha_mult;
            let inv = 1.0 - src_alpha;
            let dst = base.get_pixel(bx, by);

            let blended = image::Rgba([
                (src[0] as f32 * src_alpha + dst[0] as f32 * inv) as u8,
                (src[1] as f32 * src_alpha + dst[1] as f32 * inv) as u8,
                (src[2] as f32 * src_alpha + dst[2] as f32 * inv) as u8,
                255,
            ]);

            base.put_pixel(bx, by, blended);
        }
    }
}

/// Download an image from a URL.
async fn download_sticker_image(url: &str) -> Result<DynamicImage> {
    tracing::debug!("[STICKER] Downloading sticker from: {}", url);

    let response = reqwest::get(url).await?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "HTTP {} when downloading sticker",
            response.status()
        ));
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !content_type.starts_with("image/") {
        return Err(anyhow!(
            "URL did not return an image (content-type: {})",
            content_type
        ));
    }

    let bytes = response.bytes().await?;

    if bytes.len() > 10 * 1024 * 1024 {
        return Err(anyhow!("Sticker image too large ({} bytes)", bytes.len()));
    }

    let img = image::load_from_memory(&bytes)?;

    tracing::debug!(
        "[STICKER] Downloaded sticker ({} bytes, {}x{})",
        bytes.len(),
        img.width(),
        img.height()
    );

    Ok(img)
}

/// Validate that a URL points to a valid image.
/// Used by the /add-sticker command before inserting into DB.
pub async fn validate_image_url(url: &str) -> Result<()> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(anyhow!("URL must start with http:// or https://"));
    }

    let client = reqwest::Client::new();

    let response = match client.head(url).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => {
            client
                .get(url)
                .header("Range", "bytes=0-1023")
                .send()
                .await?
        }
    };

    if !response.status().is_success() && response.status().as_u16() != 206 {
        return Err(anyhow!("URL returned HTTP {}", response.status()));
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !content_type.starts_with("image/") {
        return Err(anyhow!(
            "URL does not point to an image (content-type: {})",
            content_type
        ));
    }

    Ok(())
}
