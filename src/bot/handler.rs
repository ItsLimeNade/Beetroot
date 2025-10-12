use ab_glyph::FontArc;
use anyhow::anyhow;

use crate::utils::database::Database;
use crate::utils::nightscout::Nightscout;

#[allow(dead_code)]
pub struct Handler {
    pub nightscout_client: Nightscout,
    pub database: Database,
    pub font: FontArc,
}

impl Handler {
    pub async fn new() -> Self {
        let font_bytes = std::fs::read("assets/fonts/GeistMono-Regular.ttf")
            .map_err(|e| anyhow!("Failed to read font: {}", e))
            .unwrap();

        Handler {
            nightscout_client: Nightscout::new(),
            database: Database::new().await.unwrap(),
            font: FontArc::try_from_vec(font_bytes)
                .map_err(|_| anyhow!("Failed to parse font"))
                .unwrap(),
        }
    }
}
