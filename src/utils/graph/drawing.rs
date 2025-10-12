use ab_glyph::PxScale;
use image::{Rgba, RgbaImage};
use imageproc::drawing::{draw_filled_circle_mut, draw_polygon_mut, draw_text_mut};
use imageproc::point::Point;

use super::types::PrefUnit;
use crate::bot::Handler;
use crate::utils::nightscout::Entry;

/// Draw insulin treatment (triangle)
#[allow(clippy::too_many_arguments)]
pub fn draw_insulin_treatment(
    img: &mut RgbaImage,
    insulin_amount: f32,
    is_microbolus: bool,
    microbolus_threshold: f32,
    x: f32,
    y: f32,
    insulin_col: Rgba<u8>,
    bg: Rgba<u8>,
    bright: Rgba<u8>,
    handler: &Handler,
) {
    let triangle_size = if is_microbolus {
        8
    } else if insulin_amount <= microbolus_threshold + 1.0 {
        12
    } else if insulin_amount <= microbolus_threshold + 5.0 {
        18
    } else {
        30
    };

    let triangle_y = y + 70.0;

    tracing::trace!(
        "[GRAPH] Drawing insulin: {:.1}u at ({:.1}, {:.1}) - size: {}",
        insulin_amount,
        x,
        triangle_y,
        triangle_size
    );

    let triangle_points = vec![
        Point::new(
            (x - triangle_size as f32) as i32,
            (triangle_y - triangle_size as f32) as i32,
        ),
        Point::new(
            (x + triangle_size as f32) as i32,
            (triangle_y - triangle_size as f32) as i32,
        ),
        Point::new(x as i32, (triangle_y + triangle_size as f32) as i32),
    ];

    draw_polygon_mut(img, &triangle_points, insulin_col);

    if !is_microbolus {
        let insulin_text = format!("{:.1}u", insulin_amount);
        let text_width = insulin_text.len() as f32 * 18.0;
        let text_x = (x - text_width / 2.0) as i32;
        let text_y = (triangle_y + triangle_size as f32 + 16.0) as i32;
        let scale = PxScale::from(36.0);

        for dx in [-1, 0, 1] {
            for dy in [-1, 0, 1] {
                if dx != 0 || dy != 0 {
                    draw_text_mut(
                        img,
                        bg,
                        text_x + dx,
                        text_y + dy,
                        scale,
                        &handler.font,
                        &insulin_text,
                    );
                }
            }
        }

        draw_text_mut(
            img,
            bright,
            text_x,
            text_y,
            scale,
            &handler.font,
            &insulin_text,
        );
    }
}

/// Draw carbs treatment (circle)
pub fn draw_carbs_treatment(
    img: &mut RgbaImage,
    carbs_amount: f32,
    x: f32,
    y: f32,
    carbs_col: Rgba<u8>,
    bg: Rgba<u8>,
    handler: &Handler,
) {
    let circle_radius = if carbs_amount < 0.5 {
        8
    } else if carbs_amount <= 2.0 {
        14
    } else {
        24
    };

    tracing::trace!(
        "[GRAPH] Drawing carbs: {:.0}g at ({:.1}, {:.1})",
        carbs_amount,
        x,
        y
    );

    let carbs_y = y - 70.0;

    draw_filled_circle_mut(img, (x as i32, carbs_y as i32), circle_radius, carbs_col);

    let carbs_text = format!("{}g", carbs_amount as i32);
    let text_width = carbs_text.len() as f32 * 18.0;
    let text_x = (x - text_width / 2.0) as i32;
    let text_y = (carbs_y - circle_radius as f32 - 50.0) as i32;
    let scale = PxScale::from(36.0);

    for dx in [-1, 0, 1] {
        for dy in [-1, 0, 1] {
            if dx != 0 || dy != 0 {
                draw_text_mut(
                    img,
                    bg,
                    text_x + dx,
                    text_y + dy,
                    scale,
                    &handler.font,
                    &carbs_text,
                );
            }
        }
    }

    draw_text_mut(
        img,
        carbs_col,
        text_x,
        text_y,
        scale,
        &handler.font,
        &carbs_text,
    );
}

/// Draw glucose reading treatment (dual circle)
#[allow(clippy::too_many_arguments)]
pub fn draw_glucose_reading(
    img: &mut RgbaImage,
    glucose_value: f32,
    x: f32,
    y: f32,
    pref: PrefUnit,
    bg: Rgba<u8>,
    bright: Rgba<u8>,
    handler: &Handler,
) {
    tracing::trace!(
        "[GRAPH] Drawing glucose reading: {:.1} at ({:.1}, {:.1})",
        glucose_value,
        x,
        y
    );

    let bg_check_radius = 12;
    let grey_outline = Rgba([128u8, 128u8, 128u8, 255u8]);
    let red_inside = Rgba([220u8, 38u8, 27u8, 255u8]);

    draw_filled_circle_mut(img, (x as i32, y as i32), bg_check_radius, grey_outline);
    draw_filled_circle_mut(img, (x as i32, y as i32), bg_check_radius - 4, red_inside);

    let glucose_text = match pref {
        PrefUnit::MgDl => format!("{:.0}", glucose_value),
        PrefUnit::Mmol => format!("{:.1}", glucose_value / 18.0),
    };
    let text_width = glucose_text.len() as f32 * 16.0;
    let text_x = (x - text_width / 2.0) as i32;
    let text_y = (y - bg_check_radius as f32 - 40.0) as i32;
    let scale = PxScale::from(32.0);

    for dx in [-1, 0, 1] {
        for dy in [-1, 0, 1] {
            if dx != 0 || dy != 0 {
                draw_text_mut(
                    img,
                    bg,
                    text_x + dx,
                    text_y + dy,
                    scale,
                    &handler.font,
                    &glucose_text,
                );
            }
        }
    }

    draw_text_mut(
        img,
        bright,
        text_x,
        text_y,
        scale,
        &handler.font,
        &glucose_text,
    );
}


/// Draw glucose data points on the graph
pub fn draw_glucose_points(
    img: &mut RgbaImage,
    entries: &[Entry],
    points_px: &[(f32, f32)],
    svg_radius: i32,
    high_col: Rgba<u8>,
    low_col: Rgba<u8>,
    axis_col: Rgba<u8>,
) {
    for (i, e) in entries.iter().enumerate() {
        let (x, y) = points_px[i];
        let color = if e.sgv > 180.0 {
            high_col
        } else if e.sgv < 70.0 {
            low_col
        } else {
            axis_col
        };
        draw_filled_circle_mut(img, (x.round() as i32, y.round() as i32), svg_radius, color);
    }
}
