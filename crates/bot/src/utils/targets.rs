use cinnamon::models::profile::ProfileConfig;

const DEFAULT_LOW_MGDL: f32 = 72.0;
const DEFAULT_HIGH_MGDL: f32 = 180.0;

fn target_to_mgdl(value: Option<f64>, is_mmol: bool) -> Option<f32> {
    let value = value?;

    if !value.is_finite() || value <= 0.0 {
        return None;
    }

    let mgdl = if is_mmol { value * 18.0 } else { value };

    if !(20.0..=600.0).contains(&mgdl) {
        return None;
    }

    Some(mgdl as f32)
}

pub fn resolve_profile_targets_mgdl(store: &ProfileConfig) -> (f32, f32, bool) {
    let is_mmol = store.units.starts_with("mmol");

    let low = target_to_mgdl(store.target_low.first().map(|x| x.value), is_mmol)
        .unwrap_or(DEFAULT_LOW_MGDL);

    let high = target_to_mgdl(store.target_high.first().map(|x| x.value), is_mmol)
        .unwrap_or(DEFAULT_HIGH_MGDL);

    if high <= low {
        return (DEFAULT_LOW_MGDL, DEFAULT_HIGH_MGDL, is_mmol);
    }

    (low, high, is_mmol)
}
