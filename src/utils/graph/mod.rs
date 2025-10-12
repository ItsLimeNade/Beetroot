mod drawing;
mod helpers;
mod stickers;
mod types;

use drawing::{
    draw_carbs_treatment, draw_glucose_points, draw_glucose_reading, draw_insulin_treatment,
    draw_mbg_reading,
};
use helpers::draw_dashed_vertical_line;
use stickers::{
    StickerConfig, draw_sticker, filter_ranges_by_duration, find_sticker_position,
    identify_status_ranges, select_stickers_to_place,
};
use types::PrefUnit;

use super::database::{NightscoutInfo, Sticker};
use super::nightscout::{Entry, Profile, Treatment};
use crate::bot::Handler;
use ab_glyph::PxScale;
use anyhow::{Result, anyhow};
use chrono::Utc;
use chrono_tz::Tz;
use image::{DynamicImage, Rgba, RgbaImage};
use imageproc::drawing::{draw_line_segment_mut, draw_text_mut};
use std::io::Cursor;

#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
pub async fn draw_graph(
    entries: &[Entry],
    treatments: &[Treatment],
    profile: &Profile,
    user_settings: &NightscoutInfo,
    stickers: &[Sticker],
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

    let nightscout_client = crate::utils::nightscout::Nightscout::new();
    let entries = match nightscout_client.filter_and_clean_entries(entries, hours, user_timezone) {
        Ok(filtered) => filtered,
        Err(e) => {
            tracing::error!("[GRAPH] Failed to filter entries: {}", e);
            return Err(anyhow!(
                "No entries found within the requested {} hour time range",
                hours
            ));
        }
    };

    tracing::info!(
        "[GRAPH] After filtering and deduplication: {} entries remain",
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
    let width = 1700u32;
    let height = 1100u32;

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

    let left_margin = 160.0_f32;
    let right_margin = 80.0_f32;
    let top_margin = 80.0_f32;
    let bottom_margin = 160.0_f32;

    let plot_w = (width as f32) - left_margin - right_margin;
    let plot_h = (height as f32) - top_margin - bottom_margin;
    let plot_left = left_margin;
    let plot_top = top_margin;
    let plot_right = plot_left + plot_w;
    let plot_bottom = plot_top + plot_h;

    let plot_padding = 20.0;

    let inner_plot_left = plot_left + plot_padding;
    let inner_plot_right = plot_right - plot_padding;
    let inner_plot_top = plot_top + plot_padding;
    let inner_plot_bottom = plot_bottom - plot_padding;
    let inner_plot_w = inner_plot_right - inner_plot_left;
    let inner_plot_h = inner_plot_bottom - inner_plot_top;

    let y_label_size_primary = 40.0_f32;
    let y_label_size_secondary = 36.0_f32;
    let x_label_size_primary = 40.0_f32;
    let x_label_size_secondary = 36.0_f32;
    let primary_legend_font_size: f32 = 40.0_f32;
    let secondary_legend_font_size: f32 = 36.0_f32;

    let svg_radius: i32 = if entries.len() < 100 { 8 } else { 6 };

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

        let label_x = (plot_left - 136.0) as i32;

        match pref {
            PrefUnit::MgDl => {
                draw_text_mut(
                    &mut img,
                    bright,
                    label_x,
                    (y_px - 16.0) as i32,
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
                    (y_px + 12.0) as i32,
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
                    (y_px - 16.0) as i32,
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
                    (y_px + 12.0) as i32,
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

    let newest_time = now;
    let oldest_time = now - chrono::Duration::hours(hours as i64);

    let total_hours = hours as f32;
    tracing::info!(
        "[GRAPH] Displaying {} hours of data (as requested)",
        total_hours
    );

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

    let time_range_seconds = (newest_time.timestamp() - oldest_time.timestamp()) as f32;

    let calculate_x_position = |entry_time: chrono::DateTime<chrono_tz::Tz>| -> f32 {
        let time_from_oldest = (entry_time.timestamp() - oldest_time.timestamp()) as f32;
        let time_ratio = time_from_oldest / time_range_seconds;
        inner_plot_left + (time_ratio * inner_plot_w)
    };

    let mut label_entries = Vec::new();
    let mut last_labeled_time = oldest_time;

    for entry in entries.iter().rev() {
        let entry_time = entry.millis_to_user_timezone(user_timezone);
        let hours_since_last =
            (entry_time.timestamp() - last_labeled_time.timestamp()) as f32 / 3600.0;

        if label_entries.is_empty() || hours_since_last >= time_interval {
            label_entries.push(entry);
            last_labeled_time = entry_time;
        }
    }

    if let Some(newest_entry) = entries.first() {
        let newest_entry_time = newest_entry.millis_to_user_timezone(user_timezone);
        if label_entries.is_empty()
            || label_entries
                .iter()
                .all(|e| e.millis_to_user_timezone(user_timezone) != newest_entry_time)
        {
            label_entries.insert(0, newest_entry);
        }
    }

    if label_entries.len() > max_x_labels {
        let step = label_entries.len() / max_x_labels;
        let mut filtered = vec![label_entries[0]];
        for i in (step..label_entries.len() - step).step_by(step) {
            filtered.push(label_entries[i]);
        }
        if label_entries.len() > 1 {
            filtered.push(*label_entries.last().unwrap());
        }
        label_entries = filtered;
    }

    let min_label_distance = 160.0;
    let mut final_label_entries = Vec::new();

    for (i, &entry) in label_entries.iter().enumerate() {
        let entry_time = entry.millis_to_user_timezone(user_timezone);
        let x_center = calculate_x_position(entry_time);

        let should_include = if final_label_entries.is_empty() {
            true
        } else {
            let last_entry: &&Entry = final_label_entries.last().unwrap();
            let last_time = last_entry.millis_to_user_timezone(user_timezone);
            let last_x_center = calculate_x_position(last_time);
            (x_center - last_x_center).abs() >= min_label_distance
        };

        if should_include || (i == label_entries.len() - 1 && final_label_entries.len() >= 2) {
            if i == label_entries.len() - 1 && !final_label_entries.is_empty() {
                let last_entry: &&Entry = final_label_entries.last().unwrap();
                let last_time = last_entry.millis_to_user_timezone(user_timezone);
                let last_x_center = calculate_x_position(last_time);
                if (x_center - last_x_center).abs() < min_label_distance {
                    final_label_entries.pop();
                }
            }
            final_label_entries.push(entry);
        }
    }

    let mut drawn_day_changes: std::collections::HashSet<chrono::NaiveDate> =
        std::collections::HashSet::new();
    let mut prev_date: Option<chrono::NaiveDate> = None;

    for entry in entries.iter() {
        let entry_time = entry.millis_to_user_timezone(user_timezone);
        let current_date = entry_time.date_naive();

        if let Some(prev_d) = prev_date
            && current_date != prev_d
            && !drawn_day_changes.contains(&current_date)
        {
            drawn_day_changes.insert(current_date);
            let x_center = calculate_x_position(entry_time);

            draw_dashed_vertical_line(
                &mut img,
                x_center,
                inner_plot_top,
                inner_plot_bottom,
                darker_dim,
                6,
                12,
            );

            let date_text = entry_time.format("%m/%d").to_string();
            let text_width = (date_text.len() as f32) * 14.0;
            draw_text_mut(
                &mut img,
                dim,
                (x_center - text_width / 2.0) as i32,
                (plot_top - 30.) as i32,
                PxScale::from(28.0),
                &handler.font,
                &date_text,
            );
        }
        prev_date = Some(current_date);
    }

    for entry in final_label_entries.iter() {
        let entry_time = entry.millis_to_user_timezone(user_timezone);
        let x_center = calculate_x_position(entry_time);
        let time_label = entry_time.format("%H:%M").to_string();

        let approx_char_width = x_label_size_primary * 0.6;
        let text_w = (time_label.chars().count() as f32) * approx_char_width;
        let x_text = (x_center - text_w / 2.0).round() as i32;

        draw_text_mut(
            &mut img,
            bright,
            x_text,
            (plot_bottom + 16.0) as i32,
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
            (plot_bottom + 56.0) as i32,
            PxScale::from(x_label_size_secondary),
            &handler.font,
            &rel,
        );
    }

    let mut points_px: Vec<(f32, f32)> = Vec::with_capacity(entries.len());
    for entry in &entries {
        let entry_time = entry.millis_to_user_timezone(user_timezone);
        let x = calculate_x_position(entry_time);
        let y = project_y(entry.sgv.clamp(
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

    tracing::info!("[GRAPH] Drawing contextual stickers");

    let status_ranges = identify_status_ranges(&entries, user_timezone);
    let status_ranges = filter_ranges_by_duration(status_ranges, &entries, user_timezone);

    let mut treatment_positions: Vec<(f32, f32)> = Vec::new();

    for treatment in treatments {
        let treatment_time = if let Some(created_at) = &treatment.created_at {
            match chrono::DateTime::parse_from_rfc3339(created_at) {
                Ok(dt) => dt.with_timezone(&user_tz),
                Err(_) => continue,
            }
        } else if let Some(ts) = treatment.date.or(treatment.mills) {
            chrono::DateTime::from_timestamp_millis(ts as i64)
                .map(|dt| dt.with_timezone(&user_tz))
                .unwrap_or(now)
        } else {
            continue;
        };

        let treatment_x = calculate_x_position(treatment_time);
        let mut closest_y = inner_plot_bottom - inner_plot_h / 2.0;

        for (i, entry) in entries.iter().enumerate() {
            let entry_time = entry.millis_to_user_timezone(user_timezone);
            let time_diff = (treatment_time.timestamp() - entry_time.timestamp()).abs();

            if time_diff < i64::MAX {
                closest_y = points_px[i].1;
                break;
            }
        }

        treatment_positions.push((treatment_x, closest_y));
    }

    for (i, entry) in entries.iter().enumerate() {
        if entry.has_mbg() {
            let (x, _) = points_px[i];
            let mbg_y = project_y(entry.mbg.unwrap_or(0.0));
            treatment_positions.push((x, mbg_y));
        }
    }

    let stickers_to_place = select_stickers_to_place(stickers, &status_ranges);

    let config = StickerConfig::default();
    let mut occupied_areas: Vec<(f32, f32, f32)> = Vec::new();

    for (sticker, range) in stickers_to_place {
        if let Some((x, y)) = find_sticker_position(
            range,
            &entries,
            &points_px,
            &occupied_areas,
            &treatment_positions,
            inner_plot_left,
            inner_plot_right,
            inner_plot_top,
            inner_plot_bottom,
            &config,
        ) {
            let abs_x = inner_plot_left + x * (inner_plot_right - inner_plot_left);
            let abs_y = inner_plot_top + y * (inner_plot_bottom - inner_plot_top);
            occupied_areas.push((abs_x, abs_y, config.sticker_radius));

            if let Err(e) = draw_sticker(
                &mut img,
                sticker,
                x,
                y,
                inner_plot_left,
                inner_plot_right,
                inner_plot_top,
                inner_plot_bottom,
                handler,
                bright,
            )
            .await
            {
                tracing::warn!(
                    "[GRAPH] Failed to draw sticker {}: {}",
                    sticker.display_name,
                    e
                );
            }
        } else {
            tracing::info!(
                "[GRAPH] Skipping sticker {} due to no available space",
                sticker.display_name
            );
        }
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

        let treatment_x = calculate_x_position(treatment_time);
        let mut closest_y = inner_plot_bottom - inner_plot_h / 2.0;
        let mut min_time_diff = i64::MAX;

        for (i, entry) in entries.iter().enumerate() {
            let entry_time = entry.millis_to_user_timezone(user_timezone);
            let time_diff = (treatment_time.timestamp() - entry_time.timestamp()).abs();

            if time_diff < min_time_diff {
                min_time_diff = time_diff;
                closest_y = points_px[i].1;
            }
        }

        let closest_x = treatment_x;

        if treatment.is_insulin() {
            let insulin_amount = treatment.insulin.unwrap_or(0.0);
            let is_smb_type = treatment.type_.as_deref() == Some("SMB");
            let is_microbolus = is_smb_type || insulin_amount <= user_settings.microbolus_threshold;

            if is_microbolus && !user_settings.display_microbolus {
                continue;
            }

            draw_insulin_treatment(
                &mut img,
                insulin_amount,
                is_microbolus,
                user_settings.microbolus_threshold,
                closest_x,
                closest_y,
                insulin_col,
                bg,
                bright,
                handler,
            );
        }

        if treatment.is_carbs() {
            let carbs_amount = treatment.carbs.unwrap_or(0.0);
            draw_carbs_treatment(
                &mut img,
                carbs_amount,
                closest_x,
                closest_y,
                carbs_col,
                bg,
                handler,
            );
        }

        if treatment.is_glucose_reading()
            && let Some(glucose_str) = &treatment.glucose
            && let Ok(glucose_value) = glucose_str.parse::<f32>()
        {
            let glucose_y = project_y(glucose_value);
            draw_glucose_reading(
                &mut img,
                glucose_value,
                closest_x,
                glucose_y,
                pref,
                bg,
                bright,
                handler,
            );
        }
    }

    draw_glucose_points(
        &mut img, &entries, &points_px, svg_radius, high_col, low_col, axis_col,
    );

    let mbg_count = entries.iter().filter(|e| e.has_mbg()).count();
    tracing::info!("[GRAPH] Found {} entries with MBG values", mbg_count);

    for (i, entry) in entries.iter().enumerate() {
        if entry.has_mbg() {
            let mbg_value = entry.mbg.unwrap_or(0.0);
            let (x, _) = points_px[i];
            let mbg_y = project_y(mbg_value);

            tracing::trace!(
                "[GRAPH] Drawing MBG reading: {:.1} at ({:.1}, {:.1}) - type: {:?}",
                mbg_value,
                x,
                mbg_y,
                entry.entry_type
            );

            draw_mbg_reading(&mut img, mbg_value, x, mbg_y, pref, bg, bright, handler);
        }
    }

    let header_x = (plot_left - 144.0) as i32;
    let header_y = (plot_bottom + 60.) as i32;
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
                header_y + 36,
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
                header_y + 36,
                PxScale::from(secondary_legend_font_size),
                &handler.font,
                "mg/dL",
            );
        }
    }

    draw_text_mut(
        &mut img,
        dim,
        20,
        10,
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
