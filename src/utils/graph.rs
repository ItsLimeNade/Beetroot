use super::database::NightscoutInfo;
use super::nightscout::{Entry, Profile, Treatment};
use crate::Handler;
use ab_glyph::PxScale;
use anyhow::{Result, anyhow};
use chrono::Utc;
use chrono_tz::Tz;
use image::{DynamicImage, Rgba, RgbaImage};
use imageproc::drawing::{
    draw_filled_circle_mut, draw_line_segment_mut, draw_polygon_mut, draw_text_mut,
};
use imageproc::point::Point;
use std::io::Cursor;

fn draw_dashed_vertical_line(
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

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
enum PrefUnit {
    MgDl,
    Mmol,
}

#[allow(dead_code)]
pub fn draw_graph(
    entries: &[Entry],
    treatments: &[Treatment],
    profile: &Profile,
    user_settings: &NightscoutInfo,
    handler: &Handler,
    hours: u16,
    save_path: Option<&str>,
) -> Result<Vec<u8>> {
    tracing::info!(
        "[GRAPH] Starting graph generation for {} hours of data",
        hours
    );
    tracing::debug!(
        "[GRAPH] Received {} entries and {} treatments",
        entries.len(),
        treatments.len()
    );

    if entries.is_empty() {
        tracing::error!("[GRAPH] No entries provided");
        return Err(anyhow!("No entries provided"));
    }

    let default_profile_name = &profile.default_profile;
    let profile_store: &crate::utils::nightscout::ProfileStore =
        profile.store.get(default_profile_name).ok_or_else(|| {
            tracing::error!(
                "[GRAPH] Default profile '{}' not found",
                default_profile_name
            );
            anyhow!("Default profile not found")
        })?;

    let user_timezone = &profile_store.timezone;
    tracing::info!("[GRAPH] Using timezone: {}", user_timezone);

    let user_tz: Tz = user_timezone.parse().unwrap_or(chrono_tz::UTC);
    let now = chrono::Utc::now().with_timezone(&user_tz);
    let cutoff_time = now - chrono::Duration::hours(hours as i64);

    let filtered_entries: Vec<&Entry> = entries
        .iter()
        .filter(|entry| {
            let entry_time = entry.millis_to_user_timezone(user_timezone);
            entry_time >= cutoff_time
        })
        .collect();

    tracing::info!(
        "[GRAPH] Filtered {} entries to {} within the last {} hours",
        entries.len(),
        filtered_entries.len(),
        hours
    );

    if filtered_entries.is_empty() {
        tracing::error!("[GRAPH] No entries found within the requested time range");
        return Err(anyhow!(
            "No entries found within the requested {} hour time range",
            hours
        ));
    }

    let entries: Vec<Entry> = filtered_entries.into_iter().cloned().collect();

    let mut seen_ids = std::collections::HashSet::new();
    let mut processed_entries = Vec::new();

    for entry in entries {
        if let Some(id) = &entry.id {
            if seen_ids.contains(id) {
                tracing::debug!("[GRAPH] Removing duplicate entry with ID: {}", id);
                continue;
            }
            seen_ids.insert(id.clone());
        }

        let entry_timestamp = entry.date.or(entry.mills).unwrap_or(0);
        let entry_sgv = (entry.sgv * 100.0) as i32;

        let is_duplicate = processed_entries.iter().any(|existing: &Entry| {
            let existing_timestamp = existing.date.or(existing.mills).unwrap_or(0);
            let existing_sgv = (existing.sgv * 100.0) as i32;

            let time_diff = (entry_timestamp as i64 - existing_timestamp as i64).abs();
            let same_sgv = entry_sgv == existing_sgv;

            if time_diff <= 30000 && same_sgv {
                tracing::debug!(
                    "[GRAPH] Removing duplicate entry: SGV={:.1}, time_diff={}ms",
                    entry.sgv,
                    time_diff
                );
                true
            } else {
                false
            }
        });

        if !is_duplicate {
            processed_entries.push(entry);
        }
    }

    let entries = processed_entries;

    tracing::info!(
        "[GRAPH] After deduplication: {} entries remain",
        entries.len()
    );

    let units_str = profile_store
        .units
        .clone()
        .unwrap_or_else(|| "mg/dl".to_string())
        .to_lowercase();

    tracing::info!("[GRAPH] Using units: {}", units_str);

    let pref = if units_str == "mmol/l" || units_str == "mmol" {
        PrefUnit::Mmol
    } else {
        PrefUnit::MgDl
    };

    let num_y_labels = 8;
    let approximation = false;
    let width = 850u32;
    let height = 550u32;

    let bg = Rgba([17u8, 24u8, 28u8, 255u8]);
    let grid_col = Rgba([30u8, 41u8, 47u8, 255u8]);
    let axis_col = Rgba([148u8, 163u8, 184u8, 255u8]);
    let bright = Rgba([248u8, 250u8, 252u8, 255u8]);
    let dim = Rgba([148u8, 163u8, 184u8, 255u8]);
    let darker_dim = Rgba([98u8, 113u8, 134u8, 255u8]);
    let high_col = Rgba([255u8, 159u8, 10u8, 255u8]);
    let low_col = Rgba([255u8, 69u8, 58u8, 255u8]);
    let insulin_col = Rgba([96u8, 165u8, 250u8, 255u8]);
    let carbs_col = Rgba([251u8, 191u8, 36u8, 255u8]);
    let _glucose_reading_col = Rgba([52u8, 211u8, 153u8, 255u8]);

    let left_margin = 80.0_f32;
    let right_margin = 40.0_f32;
    let top_margin = 40.0_f32;
    let bottom_margin = 80.0_f32;

    let plot_w = (width as f32) - left_margin - right_margin;
    let plot_h = (height as f32) - top_margin - bottom_margin;
    let plot_left = left_margin;
    let plot_top = top_margin;
    let plot_right = plot_left + plot_w;
    let plot_bottom = plot_top + plot_h;

    let plot_padding = 10.0;

    let inner_plot_left = plot_left + plot_padding;
    let inner_plot_right = plot_right - plot_padding;
    let inner_plot_top = plot_top + plot_padding;
    let inner_plot_bottom = plot_bottom - plot_padding;
    let inner_plot_w = inner_plot_right - inner_plot_left;
    let inner_plot_h = inner_plot_bottom - inner_plot_top;

    let y_label_size_primary = 20.0_f32;
    let y_label_size_secondary = 18.0_f32;
    let x_label_size_primary = 20.0_f32;
    let x_label_size_secondary = 18.0_f32;
    let primary_legend_font_size: f32 = 20.0_f32;
    let secondary_legend_font_size: f32 = 18.0_f32;

    let svg_radius: i32 = if entries.len() < 100 { 4 } else { 3 };

    let (y_min, y_max) = match pref {
        PrefUnit::MgDl => {
            let max_mg = entries.iter().map(|e| e.sgv).fold(0.0_f32, |a, b| a.max(b));
            let calculated_max = ((max_mg / 10.0).ceil() * 10.0).clamp(200.0, 400.0);
            (40.0_f32, calculated_max)
        }
        PrefUnit::Mmol => {
            let max_mg = entries.iter().map(|e| e.sgv).fold(0.0_f32, |a, b| a.max(b));
            let max_mmol = max_mg / 18.0;
            let calculated_max_mmol = (max_mmol.ceil()).clamp(11.0, 22.0);
            (2.0_f32, calculated_max_mmol)
        }
    };

    tracing::info!(
        "[GRAPH] Y-axis range: {:.1} to {:.1} ({})",
        y_min,
        y_max,
        if matches!(pref, PrefUnit::MgDl) {
            "mg/dL"
        } else {
            "mmol/L"
        }
    );

    let y_range = y_max - y_min;
    let _step_size = match pref {
        PrefUnit::MgDl => {
            let ideal_step = y_range / (num_y_labels - 1) as f32;
            ((ideal_step / 20.0).round() * 20.0).max(20.0)
        }
        PrefUnit::Mmol => {
            let ideal_step = y_range / (num_y_labels - 1) as f32;
            (ideal_step.round()).max(1.0)
        }
    };

    let project_y = |value: f32| -> f32 {
        let normalized_value = match pref {
            PrefUnit::MgDl => value,
            PrefUnit::Mmol => value / 18.0,
        };
        inner_plot_bottom - ((normalized_value - y_min) / (y_max - y_min)) * inner_plot_h
    };

    let mut img = RgbaImage::from_pixel(width, height, bg);

    draw_line_segment_mut(
        &mut img,
        (plot_left, plot_top),
        (plot_left, plot_bottom),
        axis_col,
    );
    draw_line_segment_mut(
        &mut img,
        (plot_left, plot_bottom),
        (plot_right, plot_bottom),
        axis_col,
    );

    let y_values: Vec<f32> = match pref {
        PrefUnit::MgDl => {
            let step = ((y_max - y_min) / (num_y_labels - 1) as f32 / 10.0).ceil() * 10.0;
            (0..num_y_labels)
                .map(|i| (y_min + step * i as f32).round())
                .filter(|&val| val <= y_max)
                .collect()
        }
        PrefUnit::Mmol => {
            let step = ((y_max - y_min) / (num_y_labels - 1) as f32).ceil();
            (0..num_y_labels)
                .map(|i| (y_min + step * i as f32).floor())
                .filter(|&val| val <= y_max)
                .collect()
        }
    };

    for y_val in y_values.iter() {
        let y_px = match pref {
            PrefUnit::MgDl => project_y(*y_val),
            PrefUnit::Mmol => {
                inner_plot_bottom - ((*y_val - y_min) / (y_max - y_min)) * inner_plot_h
            }
        };

        if y_px > inner_plot_top && y_px < inner_plot_bottom {
            draw_line_segment_mut(
                &mut img,
                (inner_plot_left, y_px),
                (inner_plot_right, y_px),
                grid_col,
            );
        }

        let label_x = (plot_left - 68.0) as i32;

        match pref {
            PrefUnit::MgDl => {
                draw_text_mut(
                    &mut img,
                    bright,
                    label_x,
                    (y_px - 8.0) as i32,
                    PxScale::from(y_label_size_primary),
                    &handler.font,
                    &format!("{}", (*y_val as i32)),
                );

                let mmol_v = y_val / 18.0;
                let mmol_display = if approximation {
                    format!("±{:.1}", (mmol_v * 2.0).round() / 2.0)
                } else {
                    format!("{:.1}", mmol_v)
                };
                draw_text_mut(
                    &mut img,
                    dim,
                    label_x,
                    (y_px + 6.0) as i32,
                    PxScale::from(y_label_size_secondary),
                    &handler.font,
                    &mmol_display,
                );
            }
            PrefUnit::Mmol => {
                draw_text_mut(
                    &mut img,
                    bright,
                    label_x,
                    (y_px - 8.0) as i32,
                    PxScale::from(y_label_size_primary),
                    &handler.font,
                    &format!("{:.1}", y_val),
                );

                let mg_val = y_val * 18.0;
                let mg_display = if approximation {
                    format!("±{}", ((mg_val / 10.0).round() * 10.0) as i32)
                } else {
                    format!("{}", mg_val as i32)
                };
                draw_text_mut(
                    &mut img,
                    dim,
                    label_x,
                    (y_px + 6.0) as i32,
                    PxScale::from(y_label_size_secondary),
                    &handler.font,
                    &mg_display,
                );
            }
        }
    }

    if let Some(&last_y_val) = y_values.last() {
        let y_px = match pref {
            PrefUnit::MgDl => project_y(last_y_val),
            PrefUnit::Mmol => {
                inner_plot_bottom - ((last_y_val - y_min) / (y_max - y_min)) * inner_plot_h
            }
        };

        if y_px >= inner_plot_top && y_px <= inner_plot_bottom {
            let faint_grid_col = Rgba([25u8, 35u8, 41u8, 255u8]);
            draw_line_segment_mut(
                &mut img,
                (inner_plot_left, y_px),
                (inner_plot_right, y_px),
                faint_grid_col,
            );
        }
    }

    let user_tz: Tz = user_timezone.parse().unwrap_or(chrono_tz::UTC);
    let now = Utc::now().with_timezone(&user_tz);

    let oldest_entry = entries.last().unwrap();
    let newest_entry = entries.first().unwrap();

    let oldest_time = oldest_entry.millis_to_user_timezone(user_timezone);
    let newest_time = newest_entry.millis_to_user_timezone(user_timezone);

    let total_hours = (newest_time.timestamp() - oldest_time.timestamp()) as f32 / 3600.0;
    tracing::info!("[GRAPH] Data spans {:.1} hours", total_hours);

    let max_x_labels = 6;
    let time_interval = if total_hours <= 3.0 {
        0.5
    } else if total_hours <= 6.0 {
        1.0
    } else if total_hours <= 12.0 {
        2.0
    } else {
        3.0
    };

    let n = entries.len();
    let spacing_x = inner_plot_w / n as f32;

    let mut label_indices = Vec::new();
    let mut last_labeled_time = oldest_time;

    for (i, entry) in entries.iter().enumerate().rev() {
        let entry_time = entry.millis_to_user_timezone(user_timezone);
        let hours_since_last =
            (entry_time.timestamp() - last_labeled_time.timestamp()) as f32 / 3600.0;

        if i == 0 || hours_since_last >= time_interval || label_indices.is_empty() {
            label_indices.push(n - 1 - i);
            last_labeled_time = entry_time;
        }
    }

    if !label_indices.contains(&0) {
        label_indices.insert(0, 0);
    }

    label_indices.sort();
    if label_indices.len() > max_x_labels {
        let step = label_indices.len() / max_x_labels;
        let mut filtered = vec![label_indices[0]];
        for i in (step..label_indices.len() - step).step_by(step) {
            filtered.push(label_indices[i]);
        }
        if label_indices.len() > 1 {
            filtered.push(*label_indices.last().unwrap());
        }
        label_indices = filtered;
    }

    let min_label_distance = 80.0;
    let mut final_indices = Vec::new();

    for (i, &entry_idx) in label_indices.iter().enumerate() {
        let x_center = inner_plot_left + spacing_x * ((n - 1 - entry_idx) as f32 + 0.5);

        let should_include = if final_indices.is_empty() {
            true
        } else {
            let last_included_idx = final_indices.last().unwrap();
            let last_x_center =
                inner_plot_left + spacing_x * ((n - 1 - last_included_idx) as f32 + 0.5);
            (x_center - last_x_center).abs() >= min_label_distance
        };

        if should_include || (i == label_indices.len() - 1 && final_indices.len() >= 2) {
            if i == label_indices.len() - 1 && !final_indices.is_empty() {
                let last_included_idx = final_indices.last().unwrap();
                let last_x_center =
                    inner_plot_left + spacing_x * ((n - 1 - last_included_idx) as f32 + 0.5);
                if (x_center - last_x_center).abs() < min_label_distance {
                    final_indices.pop();
                }
            }
            final_indices.push(entry_idx);
        }
    }

    label_indices = final_indices;

    for (label_pos, &entry_idx) in label_indices.iter().enumerate() {
        let e = &entries[entry_idx];
        let x_center = inner_plot_left + spacing_x * ((n - 1 - entry_idx) as f32 + 0.5);
        let entry_time = e.millis_to_user_timezone(user_timezone);
        let time_label = entry_time.format("%H:%M").to_string();

        let approx_char_width = x_label_size_primary * 0.6;
        let text_w = (time_label.chars().count() as f32) * approx_char_width;
        let x_text = (x_center - text_w / 2.0).round() as i32;

        draw_text_mut(
            &mut img,
            bright,
            x_text,
            (plot_bottom + 8.0) as i32,
            PxScale::from(x_label_size_primary),
            &handler.font,
            &time_label,
        );

        let diff = now.signed_duration_since(entry_time);
        let hours_ago = diff.num_hours();
        let minutes_ago = diff.num_minutes();

        let rel = if hours_ago == 0 && minutes_ago < 30 {
            "-0h".to_string()
        } else {
            let total_minutes = diff.num_minutes() as f32;
            let rounded_hours = (total_minutes / 30.0).round() * 0.5;

            if rounded_hours.fract() == 0.0 {
                format!("-{}h", rounded_hours as i32)
            } else {
                format!("-{:.1}h", rounded_hours)
            }
        };

        let approx_w2 = (rel.chars().count() as f32) * (x_label_size_secondary * 0.6);
        let x_text2 = (x_center - approx_w2 / 2.0).round() as i32;
        draw_text_mut(
            &mut img,
            dim,
            x_text2,
            (plot_bottom + 28.0) as i32,
            PxScale::from(x_label_size_secondary),
            &handler.font,
            &rel,
        );

        if label_pos > 0 {
            let prev_entry_idx = label_indices[label_pos - 1];
            let prev_entry = &entries[prev_entry_idx];
            let prev_time = prev_entry.millis_to_user_timezone(user_timezone);

            if entry_time.date_naive() != prev_time.date_naive() {
                draw_dashed_vertical_line(
                    &mut img,
                    x_center,
                    inner_plot_top,
                    inner_plot_bottom,
                    darker_dim,
                    3,
                    6,
                );

                draw_text_mut(
                    &mut img,
                    dim,
                    x_text + 15,
                    (plot_top - 15.) as i32,
                    PxScale::from(14.0),
                    &handler.font,
                    &entry_time.format("%m/%d").to_string(),
                );
            }
        }
    }

    let mut points_px: Vec<(f32, f32)> = Vec::with_capacity(entries.len());
    for (i, e) in entries.iter().enumerate() {
        let x = inner_plot_left + spacing_x * ((n - 1 - i) as f32 + 0.5);
        let y = project_y(e.sgv.clamp(
            match pref {
                PrefUnit::MgDl => y_min,
                PrefUnit::Mmol => y_min * 18.0,
            },
            match pref {
                PrefUnit::MgDl => y_max,
                PrefUnit::Mmol => y_max * 18.0,
            },
        ));
        points_px.push((x, y));
    }

    tracing::debug!("[GRAPH] Drawing {} treatments", treatments.len());
    for treatment in treatments {
        tracing::debug!(
            "[GRAPH] Processing treatment: event_type={:?}, created_at={:?}, date={:?}, mills={:?}, insulin={:?}, carbs={:?}",
            treatment.event_type,
            treatment.created_at,
            treatment.date,
            treatment.mills,
            treatment.insulin,
            treatment.carbs
        );

        let treatment_time = if let Some(created_at) = &treatment.created_at {
            match chrono::DateTime::parse_from_rfc3339(created_at) {
                Ok(dt) => dt.with_timezone(&user_tz),
                Err(e) => {
                    tracing::warn!("[GRAPH] Failed to parse created_at '{}': {}", created_at, e);
                    continue;
                }
            }
        } else if let Some(ts) = treatment.date.or(treatment.mills) {
            chrono::DateTime::from_timestamp_millis(ts as i64)
                .map(|dt| dt.with_timezone(&user_tz))
                .unwrap_or(now)
        } else {
            tracing::warn!("[GRAPH] Treatment has no timestamp, skipping");
            continue;
        };

        let mut closest_x = inner_plot_left;
        let mut closest_y = inner_plot_bottom - inner_plot_h / 2.0;
        let mut min_time_diff = i64::MAX;

        for (i, entry) in entries.iter().enumerate() {
            let entry_time = entry.millis_to_user_timezone(user_timezone);
            let time_diff = (treatment_time.timestamp() - entry_time.timestamp()).abs();

            if time_diff < min_time_diff {
                min_time_diff = time_diff;
                closest_x = points_px[i].0;
                closest_y = points_px[i].1;
            }
        }

        if treatment.is_insulin() {
            let insulin_amount = treatment.insulin.unwrap_or(0.0);
            let is_microbolus = insulin_amount <= user_settings.microbolus_threshold;

            if is_microbolus && !user_settings.display_microbolus {
                continue;
            }

            let triangle_size = if is_microbolus {
                4
            } else if insulin_amount <= user_settings.microbolus_threshold + 1.0 {
                6
            } else if insulin_amount <= user_settings.microbolus_threshold + 5.0 {
                9
            } else {
                15
            };

            let triangle_y = closest_y + 35.0;

            tracing::trace!(
                "[GRAPH] Drawing insulin: {:.1}u at ({:.1}, {:.1}) - size: {}",
                insulin_amount,
                closest_x,
                triangle_y,
                triangle_size
            );

            let triangle_points = vec![
                Point::new(
                    (closest_x - triangle_size as f32) as i32,
                    (triangle_y - triangle_size as f32) as i32,
                ),
                Point::new(
                    (closest_x + triangle_size as f32) as i32,
                    (triangle_y - triangle_size as f32) as i32,
                ),
                Point::new(closest_x as i32, (triangle_y + triangle_size as f32) as i32),
            ];

            draw_polygon_mut(&mut img, &triangle_points, insulin_col);

            if !is_microbolus {
                let insulin_text = format!("{:.1}u", insulin_amount);
                let text_width = insulin_text.len() as f32 * 9.0;
                draw_text_mut(
                    &mut img,
                    bright,
                    (closest_x - text_width / 2.0) as i32,
                    (triangle_y + triangle_size as f32 + 8.0) as i32,
                    PxScale::from(18.0),
                    &handler.font,
                    &insulin_text,
                );
            }
        }

        if treatment.is_carbs() {
            let carbs_amount = treatment.carbs.unwrap_or(0.0);
            let circle_radius = if carbs_amount < 0.5 {
                4
            } else if carbs_amount <= 2.0 {
                7
            } else {
                12
            };

            tracing::trace!(
                "[GRAPH] Drawing carbs: {:.0}g at ({:.1}, {:.1})",
                carbs_amount,
                closest_x,
                closest_y
            );

            let carbs_y = closest_y - 35.0;

            draw_filled_circle_mut(
                &mut img,
                (closest_x as i32, carbs_y as i32),
                circle_radius,
                carbs_col,
            );

            let carbs_text = format!("{}g", carbs_amount as i32);
            let text_width = carbs_text.len() as f32 * 9.0;
            draw_text_mut(
                &mut img,
                carbs_col,
                (closest_x - text_width / 2.0) as i32,
                (carbs_y - circle_radius as f32 - 25.0) as i32,
                PxScale::from(18.0),
                &handler.font,
                &carbs_text,
            );
        }

        if treatment.is_glucose_reading()
            && let Some(glucose_str) = &treatment.glucose
            && let Ok(glucose_value) = glucose_str.parse::<f32>()
        {
            let glucose_y = project_y(glucose_value);

            tracing::trace!(
                "[GRAPH] Drawing glucose reading: {:.1} at ({:.1}, {:.1})",
                glucose_value,
                closest_x,
                glucose_y
            );

            let bg_check_radius = 6;
            let grey_outline = Rgba([128u8, 128u8, 128u8, 255u8]);
            let red_inside = Rgba([220u8, 38u8, 27u8, 255u8]);

            let bg_y = glucose_y - 25.0;

            draw_filled_circle_mut(
                &mut img,
                (closest_x as i32, bg_y as i32),
                bg_check_radius,
                grey_outline,
            );

            draw_filled_circle_mut(
                &mut img,
                (closest_x as i32, bg_y as i32),
                bg_check_radius - 2,
                red_inside,
            );

            let glucose_text = match pref {
                PrefUnit::MgDl => format!("{:.0}", glucose_value),
                PrefUnit::Mmol => format!("{:.1}", glucose_value / 18.0),
            };
            let text_width = glucose_text.len() as f32 * 8.0;
            draw_text_mut(
                &mut img,
                bright,
                (closest_x - text_width / 2.0) as i32,
                (bg_y - bg_check_radius as f32 - 20.0) as i32,
                PxScale::from(16.0),
                &handler.font,
                &glucose_text,
            );
        }
    }

    for (i, e) in entries.iter().enumerate() {
        let (x, y) = points_px[i];
        let color = if e.sgv > 180.0 {
            high_col
        } else if e.sgv < 70.0 {
            low_col
        } else {
            axis_col
        };
        draw_filled_circle_mut(
            &mut img,
            (x.round() as i32, y.round() as i32),
            svg_radius,
            color,
        );
    }

    let header_x = (plot_left - 72.0) as i32;
    let header_y = (plot_bottom + 30.) as i32;
    match pref {
        PrefUnit::MgDl => {
            draw_text_mut(
                &mut img,
                bright,
                header_x,
                header_y,
                PxScale::from(primary_legend_font_size),
                &handler.font,
                "mg/dL",
            );
            draw_text_mut(
                &mut img,
                dim,
                header_x,
                header_y + 18,
                PxScale::from(secondary_legend_font_size),
                &handler.font,
                "mmol/L",
            );
        }
        PrefUnit::Mmol => {
            draw_text_mut(
                &mut img,
                bright,
                header_x,
                header_y,
                PxScale::from(primary_legend_font_size),
                &handler.font,
                "mmol/L",
            );
            draw_text_mut(
                &mut img,
                dim,
                header_x,
                header_y + 18,
                PxScale::from(secondary_legend_font_size),
                &handler.font,
                "mg/dL",
            );
        }
    }

    draw_text_mut(
        &mut img,
        dim,
        10,
        5,
        PxScale::from(secondary_legend_font_size),
        &handler.font,
        "Beetroot",
    );

    let dyna = DynamicImage::ImageRgba8(img);
    let mut out_buf: Vec<u8> = Vec::new();
    dyna.write_to(&mut Cursor::new(&mut out_buf), image::ImageFormat::Png)
        .map_err(|e| {
            tracing::error!("[GRAPH] Failed to encode PNG: {}", e);
            anyhow!("Failed to encode PNG: {}", e)
        })?;

    if let Some(path) = save_path {
        std::fs::write(path, &out_buf).map_err(|e| {
            tracing::error!("[GRAPH] Failed to save PNG to {}: {}", path, e);
            anyhow!("Failed to save PNG to {}: {}", path, e)
        })?;
    }

    tracing::info!(
        "[GRAPH] Successfully generated graph ({} bytes)",
        out_buf.len()
    );
    Ok(out_buf)
}
