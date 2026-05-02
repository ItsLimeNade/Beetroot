pub mod overlay;

use beetroot_core::models::{Sticker, StickerCategory};
use rand::{Rng, RngExt};

/// Represents a sticker placement on the graph.
/// `entry_index` refers to the index in the glucose entries array
/// so we can later compute the (x, y) position on the rendered image.
#[derive(Debug, Clone)]
pub struct StickerPlacement {
    pub sticker: Sticker,
    pub entry_index: usize,
}

/// Glucose status derived from a single SGV value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GlucoseStatus {
    Low,
    InRange,
    High,
}

impl GlucoseStatus {
    fn from_sgv(sgv: f32, target_low: f32, target_high: f32) -> Self {
        if sgv < target_low {
            Self::Low
        } else if sgv > target_high {
            Self::High
        } else {
            Self::InRange
        }
    }

    fn to_sticker_category(self) -> StickerCategory {
        match self {
            Self::Low => StickerCategory::Low,
            Self::InRange => StickerCategory::InRange,
            Self::High => StickerCategory::High,
        }
    }
}

/// Main sticker generation function.
///
/// # Algorithm
///
/// 1. Split the glucose entries into `k` segments (k random in [3, 5]).
/// 2. For each segment, determine the dominant glucose status (Low / InRange / High)
///    using the median glucose value.
/// 3. Pick a random sticker from the matching category for each segment.
///    Falls back to "Other" if no sticker matches that category.
/// 4. De-duplicate: for contextual stickers, if pairs of identical stickers exist,
///    50% chance one of the pair gets replaced by an "Other" sticker.
///    Replacement works per-pair: if 3 identical stickers exist, only one pair is
///    processed (pick one pair, maybe replace one -> leaves 2 distinct stickers).
/// 5. Return the list of (sticker, entry_index) placements.
///
/// # Arguments
///
/// * `sgv_values` - Glucose values (in mg/dL) corresponding to entries
/// * `user_stickers` - All of the user's stickers from the DB
/// * `target_low` - Low threshold (e.g. 70.0 mg/dL)
/// * `target_high` - High threshold (e.g. 180.0 mg/dL)
pub fn generate_sticker_placements(
    sgv_values: &[f32],
    user_stickers: &[Sticker],
    target_low: f32,
    target_high: f32,
) -> Vec<StickerPlacement> {
    if sgv_values.is_empty() || user_stickers.is_empty() {
        return Vec::new();
    }

    let mut rng = rand::rng();

    let num_segments: usize = rng.random_range(3..=5);
    let segment_size = sgv_values.len() / num_segments;

    if segment_size == 0 {
        return Vec::new();
    }

    tracing::debug!(
        "[STICKER] Splitting {} entries into {} segments (~{} entries each)",
        sgv_values.len(),
        num_segments,
        segment_size
    );

    // Pre-group user stickers by category for fast lookup
    let stickers_by_cat = group_stickers_by_category(user_stickers);

    let mut placements: Vec<StickerPlacement> = Vec::with_capacity(num_segments);

    for seg_idx in 0..num_segments {
        let start = seg_idx * segment_size;
        let end = if seg_idx == num_segments - 1 {
            sgv_values.len() // last segment takes the remainder
        } else {
            (seg_idx + 1) * segment_size
        };

        let segment = &sgv_values[start..end];
        if segment.is_empty() {
            continue;
        }

        // Determine status using the median glucose value of the segment
        let status = {
            let mut sorted: Vec<f32> = segment.to_vec();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let median = sorted[sorted.len() / 2];
            GlucoseStatus::from_sgv(median, target_low, target_high)
        };

        let category = status.to_sticker_category();

        // Try picking a sticker from the matching category, fallback to Other
        let chosen_sticker = pick_random_sticker(&stickers_by_cat, category, &mut rng)
            .or_else(|| pick_random_sticker(&stickers_by_cat, StickerCategory::Other, &mut rng));

        if let Some(sticker) = chosen_sticker {
            // Pick a random entry index within this segment
            let entry_index = rng.random_range(start..end);

            placements.push(StickerPlacement {
                sticker: sticker.clone(),
                entry_index,
            });

            tracing::debug!(
                "[STICKER] Segment {}: status={:?}, picked '{}' ({}), at entry #{}",
                seg_idx,
                status,
                sticker.display_name.as_deref().unwrap_or("?"),
                category.display_name(),
                entry_index,
            );
        }
    }

    deduplicate_placements(&mut placements, &stickers_by_cat, &mut rng);

    tracing::info!(
        "[STICKER] Final placement count: {} stickers",
        placements.len()
    );

    placements
}

/// Groups stickers into a HashMap by category for O(1) category lookup.
fn group_stickers_by_category(
    stickers: &[Sticker],
) -> std::collections::HashMap<StickerCategory, Vec<&Sticker>> {
    let mut map: std::collections::HashMap<StickerCategory, Vec<&Sticker>> =
        std::collections::HashMap::new();
    for s in stickers {
        map.entry(s.category).or_default().push(s);
    }
    map
}

/// Pick a random sticker from a given category.
fn pick_random_sticker<'a>(
    stickers_by_cat: &'a std::collections::HashMap<StickerCategory, Vec<&'a Sticker>>,
    category: StickerCategory,
    rng: &mut impl Rng,
) -> Option<&'a Sticker> {
    let candidates = stickers_by_cat.get(&category)?;
    if candidates.is_empty() {
        return None;
    }
    let idx = rng.random_range(0..candidates.len());
    Some(candidates[idx])
}

/// De-duplication algorithm.
///
/// For each pair of contextual stickers that share the same `sticker_url`,
/// there is a 50% chance one of the pair gets replaced by a random "Other" sticker.
///
/// Processing is done pair-by-pair:
/// - If 2 duplicates -> 1 pair -> maybe 1 replacement.
/// - If 3 duplicates -> pick 1 pair (indices 0,1) -> maybe 1 replacement.
///   Result: at most 2 identical remain, which is intentional.
fn deduplicate_placements(
    placements: &mut Vec<StickerPlacement>,
    stickers_by_cat: &std::collections::HashMap<StickerCategory, Vec<&Sticker>>,
    rng: &mut impl Rng,
) {
    let other_stickers = stickers_by_cat.get(&StickerCategory::Other);
    let has_other = other_stickers.is_some_and(|v| !v.is_empty());

    if !has_other {
        tracing::debug!("[STICKER] No 'Other' stickers available, skipping de-duplication");
        return;
    }

    let other_stickers = other_stickers.unwrap();

    // Build a map: sticker_url -> list of indices in `placements`
    let mut url_to_indices: std::collections::HashMap<&str, Vec<usize>> =
        std::collections::HashMap::new();

    for (i, p) in placements.iter().enumerate() {
        // Only consider contextual stickers for de-duplication
        if p.sticker.category.is_contextual() {
            url_to_indices
                .entry(&p.sticker.sticker_url)
                .or_default()
                .push(i);
        }
    }

    // Collect modifications to apply after the immutable borrow ends
    let mut modifications: Vec<(usize, &Sticker)> = Vec::new();

    for (url, indices) in &url_to_indices {
        if indices.len() < 2 {
            continue; // No duplicate
        }

        tracing::debug!(
            "[STICKER] Found {} duplicates for '{}', processing one pair",
            indices.len(),
            url
        );

        // Pick the first pair (indices[0], indices[1])
        // 50% chance we replace one of them
        let should_replace: bool = rng.random_bool(0.5);

        if should_replace {
            // Pick which of the two to replace (0 or 1)
            let target_idx = indices[rng.random_range(0..2)];

            // Pick a random Other sticker
            let replacement = other_stickers[rng.random_range(0..other_stickers.len())];

            tracing::debug!(
                "[STICKER] Replacing duplicate at position {} with 'Other' sticker '{}'",
                target_idx,
                replacement.display_name.as_deref().unwrap_or("?"),
            );

            modifications.push((target_idx, replacement));
        } else {
            tracing::debug!("[STICKER] 50% roll: keeping duplicates as-is");
        }
    }

    for (idx, sticker) in modifications {
        placements[idx].sticker = sticker.clone();
    }
}
