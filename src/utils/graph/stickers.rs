use anyhow::Result;
use image::{Rgba, RgbaImage};

use super::helpers::download_sticker_image;
use super::types::GlucoseStatus;
use crate::bot::Handler;
use crate::utils::database::{Sticker, StickerCategory};
use crate::utils::nightscout::Entry;

/// Maximum number of stickers to show per graph
pub const MAX_STICKERS_PER_GRAPH: usize = 3;

/// Configuration for sticker placement
pub struct StickerConfig {
    pub sticker_radius: f32,
    pub curve_avoidance_distance: f32,
    pub treatment_avoidance_distance: f32,
    pub max_attempts: usize,
}

impl Default for StickerConfig {
    fn default() -> Self {
        Self {
            sticker_radius: 120.0,
            curve_avoidance_distance: 100.0,
            treatment_avoidance_distance: 120.0,
            max_attempts: 500,
        }
    }
}

/// Identify glucose status ranges from entries
pub fn identify_status_ranges(
    entries: &[Entry],
    _user_timezone: &str,
    target_low: f32,
    target_high: f32,
) -> Vec<(GlucoseStatus, usize, usize)> {
    tracing::info!(
        "[GRAPH] Using thresholds for status ranges: LOW={:.1} mg/dL, HIGH={:.1} mg/dL",
        target_low,
        target_high
    );

    let mut status_ranges: Vec<(GlucoseStatus, usize, usize)> = Vec::new();

    if entries.is_empty() {
        return status_ranges;
    }

    let mut current_status = GlucoseStatus::from_sgv(entries[0].sgv, target_low, target_high);
    let mut range_start = 0;

    for (i, entry) in entries.iter().enumerate().skip(1) {
        let status = GlucoseStatus::from_sgv(entry.sgv, target_low, target_high);
        if status != current_status {
            status_ranges.push((current_status, range_start, i - 1));
            current_status = status;
            range_start = i;
        }
    }
    status_ranges.push((current_status, range_start, entries.len() - 1));

    tracing::debug!(
        "[GRAPH] Identified {} glucose status ranges",
        status_ranges.len()
    );

    status_ranges
}

/// Filter ranges by duration (Low >= 0min, InRange/High >= 30min)
pub fn filter_ranges_by_duration(
    status_ranges: Vec<(GlucoseStatus, usize, usize)>,
    entries: &[Entry],
    user_timezone: &str,
) -> Vec<(GlucoseStatus, usize, usize)> {
    let mut filtered_ranges: Vec<(GlucoseStatus, usize, usize)> = Vec::new();

    for (status, start_idx, end_idx) in status_ranges {
        if start_idx < entries.len() && end_idx < entries.len() {
            let start_time = entries[start_idx].millis_to_user_timezone(user_timezone);
            let end_time = entries[end_idx].millis_to_user_timezone(user_timezone);
            let duration_minutes = ((end_time.timestamp() - start_time.timestamp()).abs()) / 60;

            let min_duration = match status {
                GlucoseStatus::Low => 0,
                GlucoseStatus::InRange | GlucoseStatus::High => 30,
            };

            if duration_minutes >= min_duration {
                filtered_ranges.push((status, start_idx, end_idx));
            } else {
                tracing::debug!(
                    "[GRAPH] Skipping {:?} range ({}min < {}min threshold)",
                    status,
                    duration_minutes,
                    min_duration
                );
            }
        }
    }

    tracing::debug!(
        "[GRAPH] After filtering: {} ranges >= duration threshold",
        filtered_ranges.len()
    );

    filtered_ranges
}

/// Select stickers to place based on glucose status ranges
pub fn select_stickers_to_place<'a>(
    stickers: &'a [Sticker],
    status_ranges: &[(GlucoseStatus, usize, usize)],
) -> Vec<(&'a Sticker, Option<(usize, usize)>)> {
    let mut stickers_to_place: Vec<(&Sticker, Option<(usize, usize)>)> = Vec::new();

    let mut stickers_by_category: std::collections::HashMap<StickerCategory, Vec<_>> =
        std::collections::HashMap::new();
    for sticker in stickers {
        stickers_by_category
            .entry(sticker.category)
            .or_insert_with(Vec::new)
            .push(sticker);
    }

    let mut status_counts: std::collections::HashMap<GlucoseStatus, usize> =
        std::collections::HashMap::new();
    for (status, _, _) in status_ranges {
        *status_counts.entry(*status).or_insert(0) += 1;
    }

    let empty_vec: Vec<&Sticker> = Vec::new();

    while stickers_to_place.len() < MAX_STICKERS_PER_GRAPH && !status_ranges.is_empty() {
        let range_idx = (rand::random::<f32>() * status_ranges.len() as f32) as usize;
        let range_idx = range_idx.min(status_ranges.len() - 1);
        let (status, start_idx, end_idx) = status_ranges[range_idx];

        let category = status.to_sticker_category();
        let contextual_stickers = stickers_by_category.get(&category).unwrap_or(&empty_vec);
        let any_stickers = stickers_by_category
            .get(&StickerCategory::Any)
            .unwrap_or(&empty_vec);

        if contextual_stickers.is_empty() && any_stickers.is_empty() {
            break;
        }

        let status_count = *status_counts.get(&status).unwrap_or(&1);
        let use_any_sticker = if status_count > 1 {
            rand::random::<f32>() < 0.3
        } else {
            false
        };

        let selected_sticker = if use_any_sticker && !any_stickers.is_empty() {
            let idx = (rand::random::<f32>() * any_stickers.len() as f32) as usize;
            any_stickers[idx.min(any_stickers.len() - 1)]
        } else if !contextual_stickers.is_empty() {
            let idx = (rand::random::<f32>() * contextual_stickers.len() as f32) as usize;
            contextual_stickers[idx.min(contextual_stickers.len() - 1)]
        } else if !any_stickers.is_empty() {
            let idx = (rand::random::<f32>() * any_stickers.len() as f32) as usize;
            any_stickers[idx.min(any_stickers.len() - 1)]
        } else {
            break;
        };

        stickers_to_place.push((selected_sticker, Some((start_idx, end_idx))));
    }

    let any_stickers = stickers_by_category
        .get(&StickerCategory::Any)
        .unwrap_or(&empty_vec);

    while stickers_to_place.len() < MAX_STICKERS_PER_GRAPH && !any_stickers.is_empty() {
        let idx = (rand::random::<f32>() * any_stickers.len() as f32) as usize;
        let sticker = any_stickers[idx.min(any_stickers.len() - 1)];
        stickers_to_place.push((sticker, None));
    }

    tracing::info!(
        "[GRAPH] Placing {} stickers (max {})",
        stickers_to_place.len(),
        MAX_STICKERS_PER_GRAPH
    );

    stickers_to_place
}

/// Find a valid position for a sticker
#[allow(clippy::too_many_arguments)]
pub fn find_sticker_position(
    range: Option<(usize, usize)>,
    entries: &[Entry],
    points_px: &[(f32, f32)],
    occupied_areas: &[(f32, f32, f32)],
    treatment_positions: &[(f32, f32)],
    inner_plot_left: f32,
    inner_plot_right: f32,
    inner_plot_top: f32,
    inner_plot_bottom: f32,
    config: &StickerConfig,
) -> Option<(f32, f32)> {
    let inner_plot_w = inner_plot_right - inner_plot_left;
    let inner_plot_h = inner_plot_bottom - inner_plot_top;

    let target_entry_idx = if let Some((start_idx, end_idx)) = range {
        let range_size = end_idx - start_idx + 1;
        let offset = (rand::random::<f32>() * range_size as f32) as usize;
        start_idx + offset.min(range_size - 1)
    } else {
        let idx = (rand::random::<f32>() * entries.len() as f32) as usize;
        idx.min(entries.len() - 1)
    };

    for attempts in 0..config.max_attempts {
        let search_expansion = (attempts as f32 / config.max_attempts as f32) * 0.5;

        let (x, y) = if range.is_some() {
            let target_x = points_px[target_entry_idx].0;
            let target_y = points_px[target_entry_idx].1;
            let target_x_normalized = (target_x - inner_plot_left) / inner_plot_w;
            let target_y_normalized = (target_y - inner_plot_top) / inner_plot_h;

            let base_vertical = 0.20;
            let vertical_offset = if rand::random::<bool>() {
                -(base_vertical + search_expansion)
            } else {
                base_vertical + search_expansion
            };
            let horizontal_range = 0.15 + search_expansion;
            let horizontal_offset = (rand::random::<f32>() - 0.5) * horizontal_range;

            (
                (target_x_normalized + horizontal_offset).clamp(0.05, 0.95),
                (target_y_normalized + vertical_offset).clamp(0.05, 0.95),
            )
        } else if attempts < config.max_attempts / 2 {
            match rand::random::<u8>() % 4 {
                0 => (
                    rand::random::<f32>() * 0.3 + 0.1,
                    rand::random::<f32>() * 0.3 + 0.1,
                ),
                1 => (
                    rand::random::<f32>() * 0.3 + 0.6,
                    rand::random::<f32>() * 0.3 + 0.1,
                ),
                2 => (
                    rand::random::<f32>() * 0.3 + 0.1,
                    rand::random::<f32>() * 0.3 + 0.6,
                ),
                _ => (
                    rand::random::<f32>() * 0.3 + 0.6,
                    rand::random::<f32>() * 0.3 + 0.6,
                ),
            }
        } else {
            (
                rand::random::<f32>() * 0.6 + 0.2,
                rand::random::<f32>() * 0.6 + 0.2,
            )
        };

        let abs_x = inner_plot_left + x * inner_plot_w;
        let abs_y = inner_plot_top + y * inner_plot_h;

        let has_collision = occupied_areas.iter().any(|(ox, oy, r)| {
            let distance = ((abs_x - ox).powi(2) + (abs_y - oy).powi(2)).sqrt();
            distance < (config.sticker_radius + r)
        });

        let too_close_to_curve = points_px.iter().any(|(px, py)| {
            let distance = ((abs_x - px).powi(2) + (abs_y - py).powi(2)).sqrt();
            distance < config.curve_avoidance_distance
        });

        let too_close_to_treatments = treatment_positions.iter().any(|(tx, ty)| {
            let distance = ((abs_x - tx).powi(2) + (abs_y - ty).powi(2)).sqrt();
            distance < config.treatment_avoidance_distance
        });

        if !has_collision && !too_close_to_curve && !too_close_to_treatments {
            return Some((x, y));
        }
    }

    None
}

/// Draw a single sticker on the graph
#[allow(clippy::too_many_arguments)]
pub async fn draw_sticker(
    img: &mut RgbaImage,
    sticker: &Sticker,
    x: f32,
    y: f32,
    inner_plot_left: f32,
    inner_plot_right: f32,
    inner_plot_top: f32,
    inner_plot_bottom: f32,
    _handler: &Handler,
) -> Result<()> {
    let inner_plot_w = inner_plot_right - inner_plot_left;
    let inner_plot_h = inner_plot_bottom - inner_plot_top;

    tracing::debug!(
        "[GRAPH] Drawing sticker: {} at ({:.2}, {:.2})",
        sticker.file_name,
        x,
        y
    );

    let sticker_img = if sticker.file_name.starts_with("http") {
        download_sticker_image(&sticker.file_name).await?
    } else {
        image::open(&sticker.file_name)?
    };

    let sticker_rgba = sticker_img.to_rgba8();
    let (sticker_w, sticker_h) = sticker_rgba.dimensions();

    let sticker_x = (inner_plot_left + x * inner_plot_w) as i32;
    let sticker_y = (inner_plot_top + y * inner_plot_h) as i32;

    let max_size = 200;
    let scale_factor = if sticker_w > sticker_h {
        max_size as f32 / sticker_w as f32
    } else {
        max_size as f32 / sticker_h as f32
    };
    let new_w = (sticker_w as f32 * scale_factor) as u32;
    let new_h = (sticker_h as f32 * scale_factor) as u32;

    let resized_sticker = image::imageops::resize(
        &sticker_rgba,
        new_w,
        new_h,
        image::imageops::FilterType::Lanczos3,
    );

    let start_x = (sticker_x - new_w as i32 / 2).max(0);
    let start_y = (sticker_y - new_h as i32 / 2).max(0);

    for y in 0..new_h {
        for x in 0..new_w {
            let img_x = start_x + x as i32;
            let img_y = start_y + y as i32;

            if img_x >= 0 && img_x < img.width() as i32 && img_y >= 0 && img_y < img.height() as i32
            {
                let sticker_pixel = resized_sticker.get_pixel(x, y);

                if sticker_pixel[3] > 128 {
                    let base_alpha = sticker_pixel[3] as f32 / 255.0;
                    let alpha = base_alpha * 0.8;
                    let bg_pixel = img.get_pixel(img_x as u32, img_y as u32);

                    let darkened_r = (sticker_pixel[0] as f32 * 0.8) as u8;
                    let darkened_g = (sticker_pixel[1] as f32 * 0.8) as u8;
                    let darkened_b = (sticker_pixel[2] as f32 * 0.8) as u8;

                    let blended = Rgba([
                        ((darkened_r as f32 * alpha) + (bg_pixel[0] as f32 * (1.0 - alpha))) as u8,
                        ((darkened_g as f32 * alpha) + (bg_pixel[1] as f32 * (1.0 - alpha))) as u8,
                        ((darkened_b as f32 * alpha) + (bg_pixel[2] as f32 * (1.0 - alpha))) as u8,
                        255,
                    ]);

                    img.put_pixel(img_x as u32, img_y as u32, blended);
                }
            }
        }
    }

    tracing::trace!(
        "[GRAPH] Successfully drew sticker {} at ({}, {})",
        sticker.file_name,
        sticker_x,
        sticker_y
    );

    // let debug_label = match sticker.category {
    //     StickerCategory::Low => "low_sticker",
    //     StickerCategory::InRange => "inrange_sticker",
    //     StickerCategory::High => "high_sticker",
    //     StickerCategory::Any => "any_sticker",
    // };

    // let label_y = (sticker_y + new_h as i32 / 2 + 50).min(img.height() as i32 - 20);
    // let label_x = (sticker_x - 60).max(10);

    // draw_text_mut(
    //     img,
    //     bright,
    //     label_x,
    //     label_y,
    //     PxScale::from(32.0),
    //     &handler.font,
    //     debug_label,
    // );

    Ok(())
}
