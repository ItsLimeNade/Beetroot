use anyhow::{Result, anyhow};
use beetroot_core::models::Sticker as DbSticker;
use bonbon::prelude::{Sticker as BonbonSticker, StickerCategory as BonbonCategory, StickerSource};
use std::collections::HashMap;

/// Maximum allowed sticker size (10 MiB).
const MAX_STICKER_BYTES: usize = 10 * 1024 * 1024;

/// Maximum decoded sticker dimension on either axis. The byte cap bounds the
/// download but not the pixels it expands to, so a tiny highly-compressible
/// image could still blow up memory when decoded; this rejects such bombs.
const MAX_STICKER_DIMENSION: u32 = 4096;

/// Convert our DB enum to bonbon's enum.
pub fn to_bonbon_category(cat: beetroot_core::models::StickerCategory) -> BonbonCategory {
    use beetroot_core::models::StickerCategory as C;
    match cat {
        C::InRange => BonbonCategory::InRange,
        C::Low => BonbonCategory::Low,
        C::High => BonbonCategory::High,
        C::FastRise => BonbonCategory::FastRise,
        C::FastDrop => BonbonCategory::FastDrop,
        C::Background => BonbonCategory::Background,
    }
}

/// Download every unique sticker URL once, then build the list of
/// `bonbon::Sticker` instances ready for `StickerSet::with_stickers`.
pub async fn load_bonbon_stickers(db_stickers: &[DbSticker]) -> Vec<BonbonSticker> {
    if db_stickers.is_empty() {
        return Vec::new();
    }

    let unique_urls: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        db_stickers
            .iter()
            .filter(|s| seen.insert(s.sticker_url.clone()))
            .map(|s| s.sticker_url.clone())
            .collect()
    };

    let downloads = unique_urls.iter().map(|u| download_bytes(u));
    let results = futures::future::join_all(downloads).await;

    let mut cache: HashMap<String, Vec<u8>> = HashMap::new();
    for (url, res) in unique_urls.into_iter().zip(results) {
        match res {
            Ok(bytes) => {
                cache.insert(url, bytes);
            }
            Err(e) => tracing::warn!("[STICKER] Failed to fetch '{}': {}", url, e),
        }
    }

    db_stickers
        .iter()
        .filter_map(|s| {
            let bytes = cache.get(&s.sticker_url)?.clone();
            Some(BonbonSticker::new(
                StickerSource::from_bytes(bytes),
                to_bonbon_category(s.category),
            ))
        })
        .collect()
}

async fn download_bytes(url: &str) -> Result<Vec<u8>> {
    let parsed = url::Url::parse(url).map_err(|e| anyhow!("invalid sticker URL: {e}"))?;
    crate::utils::net::check_public_url(&parsed).map_err(|e| anyhow!(e))?;

    let response = crate::utils::net::guarded_client()
        .get(parsed)
        .send()
        .await?;
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
    if looks_like_webpage(content_type) {
        return Err(anyhow!(
            "URL returned a web page, not an image (content-type: {content_type})"
        ));
    }

    let bytes = response.bytes().await?;
    if bytes.len() > MAX_STICKER_BYTES {
        return Err(anyhow!("Sticker image too large ({} bytes)", bytes.len()));
    }

    let (width, height) = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|e| anyhow!("could not read sticker image: {e}"))?
        .into_dimensions()
        .map_err(|e| anyhow!("invalid sticker image: {e}"))?;
    if width > MAX_STICKER_DIMENSION || height > MAX_STICKER_DIMENSION {
        return Err(anyhow!(
            "sticker dimensions too large ({width}x{height}, max {MAX_STICKER_DIMENSION})"
        ));
    }

    Ok(bytes.to_vec())
}

/// Validate that a URL points to a valid image. Used by `/add-sticker`
/// before inserting into the DB.
pub async fn validate_image_url(url: &str) -> Result<()> {
    let parsed = url::Url::parse(url).map_err(|e| anyhow!("invalid URL: {e}"))?;
    crate::utils::net::check_public_url(&parsed).map_err(|e| anyhow!(e))?;

    let client = crate::utils::net::guarded_client();

    let response = match client.head(parsed.clone()).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => {
            client
                .get(parsed)
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

    if looks_like_webpage(content_type) {
        return Err(anyhow!(
            "That link looks like a web page, not a direct image. Open the image \
             itself and copy its address (it should end in .png, .jpg, .webp or .gif)."
        ));
    }

    Ok(())
}

/// True when a content-type clearly indicates a web page or other text document
/// rather than an image. Kept deliberately narrow so images with a generic or
/// absent content-type still pass; the real image decode is the final check.
fn looks_like_webpage(content_type: &str) -> bool {
    let ct = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    ct.starts_with("text/")
        || ct == "application/json"
        || ct == "application/xml"
        || ct == "application/xhtml+xml"
}
