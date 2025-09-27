use super::nightscout::{Entry, Profile};
use ab_glyph::{FontArc, PxScale};
use anyhow::{Result, anyhow};
use chrono::Utc;
use chrono_tz::Tz;
use image::{DynamicImage, Rgba, RgbaImage};
use imageproc::drawing::{draw_filled_circle_mut, draw_line_segment_mut, draw_text_mut};
use std::io::Cursor;

/// Which unit the user prefers to see first on the Y axis
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
enum PrefUnit {
    MgDl,
    Mmol,
}

/// Draw the graph to PNG bytes. Optionally saves to `save_path`.
///
/// Returns: Vec<u8> (PNG bytes)
#[allow(dead_code)]
pub fn draw_graph(
    entries: &[Entry],
    profile: &Profile,
    save_path: Option<&str>,
) -> Result<Vec<u8>> {
    // I'm gonna try my best to make this file readable. Often times when I look at image generations code I never undderstand
    // a single line so I am going to comment and label every part that might confuse people!
    // Have fun reading :)

    if entries.is_empty() {
        return Err(anyhow!("No entries provided"));
    }

    let default_profile_name = &profile.default_profile;
    let profile_store = profile
        .store
        .get(default_profile_name)
        .ok_or_else(|| anyhow!("Default profile not found"))?;

    let user_timezone = &profile_store.timezone;
    println!("{:#?}", profile_store.units.to_lowercase());
    let pref = if profile_store.units.to_lowercase() == "mmol/l" {
        PrefUnit::Mmol
    } else {
        PrefUnit::MgDl
    };

    // ---------- Configuration ----------
    let num_y_labels = 8; // Numbers of lines inside the graph to use as reference, also dictates how many X axis labels are drawn.
    let approximation = false; // Enables the approximations with the ± sign
    let width = 850u32;
    let height = 550u32;

    // Palette
    let bg = Rgba([15u8, 18u8, 15u8, 255u8]);
    let grid_col = Rgba([51u8, 61u8, 61u8, 255u8]);
    let axis_col = Rgba([154u8, 184u8, 184u8, 255u8]);
    let bright = Rgba([220u8, 220u8, 220u8, 255u8]);
    let dim = Rgba([150u8, 150u8, 150u8, 255u8]);
    let high_col = Rgba([255u8, 184u8, 0u8, 255u8]); // >180
    let low_col = Rgba([255u8, 64u8, 64u8, 255u8]); // <70

    // Margins and plotting rectangle
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

    let plot_padding = 10.0; // Padding inside the plot so the graph does not overlap with the plot's border making it look wierd.

    // Inner plot area with padding
    let inner_plot_left = plot_left + plot_padding;
    let inner_plot_right = plot_right - plot_padding;
    let inner_plot_top = plot_top + plot_padding;
    let inner_plot_bottom = plot_bottom - plot_padding;
    let inner_plot_w = inner_plot_right - inner_plot_left;
    let inner_plot_h = inner_plot_bottom - inner_plot_top;

    let font_bytes = std::fs::read("assets/fonts/GeistMono-Regular.ttf")
        .map_err(|e| anyhow!("Failed to read font: {}", e))?;
    let font = FontArc::try_from_vec(font_bytes).map_err(|_| anyhow!("Failed to parse font"))?;

    // Text sizes and stuff
    let y_label_size_primary = 20.0_f32;
    let y_label_size_secondary = 18.0_f32;
    let x_label_size_primary = 20.0_f32;
    let x_label_size_secondary = 18.0_f32;
    let primary_legend_font_size: f32 = 20.0_f32;
    let secondary_legend_font_size: f32 = 18.0_f32;

    // When more than 100 readings shown, this makes the svg point radius a bit smaller for readablility!
    let svg_radius: i32 = if entries.len() < 100 { 4 } else { 3 };

    // ---------- Compute Y scaling ----------
    // We're making the graph start at 40 mg/dl (2.2 mmol) and go up to y_max
    let y_min = 40.0_f32;
    let max_mg = entries.iter().map(|e| e.sgv).fold(0.0_f32, |a, b| a.max(b));
    // round up to nearest 50 mg/dL, but make sure it's at least 200 and making sure the graph will never excede 400mg/dl :3
    let y_max = ((max_mg / 50.0).ceil() * 50.0).clamp(200., 400.);

    // Calculate exact step to get exactly num_y_labels labels with even numbers
    let y_range = y_max - y_min;
    let _ = match pref {
        PrefUnit::MgDl => {
            let ideal_step = y_range / (num_y_labels - 1) as f32;
            ((ideal_step / 20.0).round() * 20.0).max(20.0)
        }
        PrefUnit::Mmol => {
            let y_min_mmol = (y_min / 18.0).round();
            let y_max_mmol = (y_max / 18.0).round();
            let mmol_range = y_max_mmol - y_min_mmol;
            let ideal_mmol_step = mmol_range / (num_y_labels - 1) as f32;
            let mmol_step = (ideal_mmol_step.round()).max(1.0);
            mmol_step * 18.0
        }
    };

    let _ = match pref {
        PrefUnit::MgDl => (y_min / 20.0).round() * 20.0,
        PrefUnit::Mmol => ((y_min / 18.0).round()) * 18.0,
    };

    // helper to project mg value to pixel Y (now based on inner plot area)
    // Projects a data-space y value (`mg`) into a screen-space y coordinate within the inner plot area.
    //
    // The value is normalized from the range `[y_min, y_max]` to the plot height `inner_plot_h` and
    // inverted so that larger data values appear higher on the screen (smaller y), offset from
    // `inner_plot_bottom`.
    let project_y =
        |mg: f32| -> f32 { inner_plot_bottom - ((mg - y_min) / (y_max - y_min)) * inner_plot_h };

    // ---------- Start image ----------
    let mut img = RgbaImage::from_pixel(width, height, bg);

    // Draw axis lines (left and bottom)
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

    // Calculate exact step to get exactly num_y_labels labels evenly distributed
    let y_range = y_max - y_min;
    let step_mg = y_range / (num_y_labels - 1) as f32;

    // Generate exactly num_y_labels values evenly spaced from y_min to y_max
    let y_values: Vec<f32> = (0..num_y_labels)
        .map(|i| y_min + step_mg * i as f32)
        .collect();

    // ---------- Horizontal grid lines + Y labels ----------
    for y_val in y_values.iter() {
        let y_px = project_y(*y_val);

        // Only draw grid lines that don't overlap with plot borders
        if y_px > inner_plot_top && y_px < inner_plot_bottom {
            draw_line_segment_mut(
                &mut img,
                (inner_plot_left, y_px),
                (inner_plot_right, y_px),
                grid_col,
            );
        }

        // Labels: primary (preferred) bright, secondary dim below/above
        let mmol_v = y_val / 18.0;

        // Where to place labels (left side)
        let label_x = (plot_left - 68.0) as i32;

        match pref {
            PrefUnit::MgDl => {
                // Primary: mg/dL (always accurate, no ± sign)
                // Round to nearest even number for display
                let mg_val = (*y_val / 20.0).round() * 20.0;
                draw_text_mut(
                    &mut img,
                    bright,
                    label_x,
                    (y_px - 8.0) as i32,
                    PxScale::from(y_label_size_primary),
                    &font,
                    &format!("{}", mg_val as i32),
                );

                // Secondary: mmol (approximated to nearest 0.5 with ± sign)
                let mmol_rounded = (mmol_v * 2.0).round() / 2.0;
                let mmol_display = if approximation {
                    format!("±{:.1}", mmol_rounded)
                } else {
                    format!("{:.1}", mmol_v)
                };
                draw_text_mut(
                    &mut img,
                    dim,
                    label_x,
                    (y_px + 6.0) as i32,
                    PxScale::from(y_label_size_secondary),
                    &font,
                    &mmol_display,
                );
            }
            PrefUnit::Mmol => {
                let mmol_val = mmol_v.round();
                draw_text_mut(
                    &mut img,
                    bright,
                    label_x,
                    (y_px - 8.0) as i32,
                    PxScale::from(y_label_size_primary),
                    &font,
                    &format!("{}", mmol_val as i32),
                );

                let mg_rounded = (*y_val / 10.0).round() * 10.0;
                let mg_display = if approximation {
                    format!("±{}", mg_rounded as i32)
                } else {
                    format!("{}", *y_val as i32)
                };
                draw_text_mut(
                    &mut img,
                    dim,
                    label_x,
                    (y_px + 6.0) as i32,
                    PxScale::from(y_label_size_secondary),
                    &font,
                    &mg_display,
                );
            }
        }
    }

    // ---------- X axis labels ----------
    // Show max 8 labels, entries[0] is newest (rightmost), entries[n-1] is oldest (leftmost)
    let n = entries.len();
    let max_x_labels = 8.min(n);
    let spacing_x = inner_plot_w / n as f32;
    let user_tz: Tz = user_timezone.parse().unwrap_or(chrono_tz::UTC);
    let now = Utc::now().with_timezone(&user_tz);

    // Always include the first entry (most recent, should be rightmost)
    let mut label_indices = vec![0];

    // Add additional evenly spaced indices if we have room for more labels
    if max_x_labels > 1 {
        let step = if max_x_labels == 1 {
            1
        } else {
            (n as f32 / (max_x_labels - 1) as f32).ceil() as usize
        };

        // Add other indices, avoiding duplicates
        for i in (step..n).step_by(step).take(max_x_labels - 1) {
            if !label_indices.contains(&i) {
                label_indices.push(i);
            }
        }
    }

    label_indices.sort();

    for &i in &label_indices {
        let e = &entries[i];
        // entries[0] is newest (rightmost), entries[n-1] is oldest (leftmost)
        // So x position: i=0 gets rightmost position, i=n-1 gets leftmost position
        let x_center = inner_plot_left + spacing_x * ((n - 1 - i) as f32 + 0.5);
        let time_label = e
            .millis_to_user_timezone(user_timezone)
            .format("%H:%M")
            .to_string();

        let approx_char_width = x_label_size_primary * 0.6;
        let text_w = (time_label.chars().count() as f32) * approx_char_width;
        let x_text = (x_center - text_w / 2.0).round() as i32;

        draw_text_mut(
            &mut img,
            bright,
            x_text,
            (plot_bottom + 8.0) as i32,
            PxScale::from(x_label_size_primary),
            &font,
            &time_label,
        );

        let diff = now.signed_duration_since(e.millis_to_user_timezone(user_timezone));
        let hours = diff.num_hours().max(0);
        let rel = format!("-{}h", hours);
        let approx_w2 = (rel.chars().count() as f32) * (x_label_size_secondary * 0.6);
        let x_text2 = (x_center - approx_w2 / 2.0).round() as i32;
        draw_text_mut(
            &mut img,
            dim,
            x_text2,
            (plot_bottom + 28.0) as i32,
            PxScale::from(x_label_size_secondary),
            &font,
            &rel,
        );
    }
    // ---------- Data line + points ----------
    // Compute pixel coordinates with correct positioning
    let mut points_px: Vec<(f32, f32)> = Vec::with_capacity(entries.len());
    for (i, e) in entries.iter().enumerate() {
        // entries[0] is newest (rightmost), entries[n-1] is oldest (leftmost)
        let x = inner_plot_left + spacing_x * ((n - 1 - i) as f32 + 0.5);
        let y = project_y(e.sgv.clamp(y_min, y_max));
        points_px.push((x, y));
    }

    // draw points
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

    // ---------- Y-axis header (preferred unit first, other unit below it) ----------
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
                &font,
                "mg/dL",
            );
            draw_text_mut(
                &mut img,
                dim,
                header_x,
                header_y + 18,
                PxScale::from(secondary_legend_font_size),
                &font,
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
                &font,
                "mmol/L",
            );
            draw_text_mut(
                &mut img,
                dim,
                header_x,
                header_y + 18,
                PxScale::from(secondary_legend_font_size),
                &font,
                "mg/dL",
            );
        }
    }

    // Logo :3
    draw_text_mut(
        &mut img,
        dim,
        10,
        5,
        PxScale::from(secondary_legend_font_size),
        &font,
        "Beetroot",
    );

    let dyna = DynamicImage::ImageRgba8(img);
    let mut out_buf: Vec<u8> = Vec::new();
    dyna.write_to(&mut Cursor::new(&mut out_buf), image::ImageFormat::Png)
        .map_err(|e| anyhow!("Failed to encode PNG: {}", e))?;

    if let Some(path) = save_path {
        std::fs::write(path, &out_buf)
            .map_err(|e| anyhow!("Failed to save PNG to {}: {}", path, e))?;
    }

    Ok(out_buf)
}
