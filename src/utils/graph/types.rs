/// Preference unit for glucose display
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum PrefUnit {
    MgDl,
    Mmol,
}

/// Glucose status ranges for contextual sticker placement
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GlucoseStatus {
    Low,
    InRange,
    High,
}

impl GlucoseStatus {
    pub fn from_sgv(sgv: f32) -> Self {
        if sgv < 70.0 {
            Self::Low
        } else if sgv > 180.0 {
            Self::High
        } else {
            Self::InRange
        }
    }

    pub fn to_sticker_category(self) -> crate::utils::database::StickerCategory {
        use crate::utils::database::StickerCategory;
        match self {
            Self::Low => StickerCategory::Low,
            Self::InRange => StickerCategory::InRange,
            Self::High => StickerCategory::High,
        }
    }
}
