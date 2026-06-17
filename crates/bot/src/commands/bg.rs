use crate::data::{Context, Error};
use crate::utils::duration_parser::parse_ago_duration;
use crate::utils::emojis;
use crate::utils::sticker_assets;
use crate::utils::theme_assets;
use bonbon::prelude::*;
use cinnamon::models::properties::PropertyType;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder};
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateAttachment, CreateEmbed, CreateEmbedFooter};

#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("bg")]
/// Checks your current blood glucose, IOB and COB.
pub async fn bg(
    ctx: Context<'_>,
    #[description = "Target user"] user: Option<serenity::User>,
    #[description = "Look back in time (e.g. '30s', '2h', '1d', '1w', '1mo', '1y', '1h30m')"]
    #[rename = "at"]
    at_str: Option<String>,
) -> Result<(), Error> {
    let target_user = user.as_ref().unwrap_or(ctx.author());
    let target_id = target_user.id;

    let user_data = get_db_user!(ctx, target_id.get());

    tracing::debug!(
        target_user = %crate::logging::redact(target_id.get()),
        image_mode = user_data.bg_image_mode,
        "bg dispatch"
    );

    check_privacy!(ctx, target_id, user_data);

    let client = get_nightscout_client!(ctx, user_data);

    let lookback = if let Some(ref s) = at_str {
        match parse_ago_duration(s) {
            Some(d) => Some(d),
            None => {
                send_error!(
                    ctx,
                    "Invalid Time",
                    "Could not parse the time. Use formats like `30s`, `30m`, `2h`, `1d`, `1w`, `1mo`, `1y`, or combinations like `1h30m`."
                );
                return Ok(());
            }
        }
    } else {
        None
    };

    crate::tips::safe_defer(ctx).await?;

    let now = chrono::Utc::now();

    if user_data.bg_image_mode {
        let sparkline_entries = if let Some(ago) = lookback {
            let target_time = now - ago;
            let window_start = target_time - chrono::Duration::hours(3);
            match client
                .sgv()
                .get()
                .from(window_start)
                .to(target_time + chrono::Duration::minutes(5))
                .limit(50)
                .send()
                .await
            {
                Ok(e) if !e.is_empty() => e,
                _ => {
                    send_error!(
                        ctx,
                        "No Data",
                        format!(
                            "No glucose data found around **{}** ago.",
                            at_str.as_deref().unwrap_or("?")
                        )
                    );
                    return Ok(());
                }
            }
        } else {
            match client.sgv().get().limit(36).send().await {
                Ok(e) if !e.is_empty() => e,
                _ => {
                    send_error!(
                        ctx,
                        "Fetch Error",
                        "Could not fetch blood glucose data. Please check your URL."
                    );
                    return Ok(());
                }
            }
        };

        let properties_fut = client
            .properties()
            .get()
            .only(&[PropertyType::Iob, PropertyType::Cob])
            .send();
        let profiles_builder = client.profiles();
        let profile_fut = profiles_builder.get();
        let (properties_result, profile_result) = tokio::join!(properties_fut, profile_fut);

        tracing::debug!(
            sparkline_entries = sparkline_entries.len(),
            "bg/image data fetched"
        );
        crate::log_medical!(
            sparkline = ?sparkline_entries,
            properties = ?properties_result,
            profiles = ?profile_result,
            "bg/image raw data dump"
        );

        let (target_low, target_high, is_mmol) = if let Ok(profiles) = profile_result {
            if let Some(profile) = profiles.first() {
                if let Some(store) = profile.store.get(&profile.default_profile_name) {
                    let low = store.target_low.first().map(|x| x.value).unwrap_or(4.0);
                    let high = store.target_high.first().map(|x| x.value).unwrap_or(10.0);
                    let mmol = store.units.starts_with("mmol");
                    if mmol {
                        (low * 18.0, high * 18.0, true)
                    } else {
                        (low, high, false)
                    }
                } else {
                    (72.0, 180.0, false)
                }
            } else {
                (72.0, 180.0, false)
            }
        } else {
            (72.0, 180.0, false)
        };

        let mut sorted = sparkline_entries;
        sorted.sort_by_key(|e| e.date);

        let point_count = sorted.len();
        let entry = sorted.last().unwrap();
        let prev = if point_count >= 2 {
            sorted.get(point_count - 2)
        } else {
            None
        };
        let delta = prev.map(|p| entry.sgv as f64 - p.sgv as f64).unwrap_or(0.0);

        let entry_time = chrono::DateTime::parse_from_rfc3339(&entry.date_string)
            .unwrap_or_else(|_| now.into())
            .with_timezone(&chrono::Utc);
        let duration = now.signed_duration_since(entry_time);
        let age_str = if duration.num_minutes() < 60 {
            format!("{} min ago", duration.num_minutes())
        } else {
            format!("{} h ago", duration.num_hours())
        };

        let current_status = if (entry.sgv as f64) < target_low {
            GlucoseStatus::Low
        } else if (entry.sgv as f64) > target_high {
            GlucoseStatus::High
        } else {
            GlucoseStatus::InRange
        };

        let iob_str = properties_result.as_ref().ok().and_then(|props| {
            props
                .iob
                .as_ref()
                .filter(|i| i.iob > 0.0)
                .map(|i| format!("IOB {:.2}u", i.iob))
        });
        let cob_str = properties_result.as_ref().ok().and_then(|props| {
            props
                .cob
                .as_ref()
                .filter(|c| c.cob > 0.0)
                .map(|c| format!("COB {:.0}g", c.cob))
        });

        let sparkline_points: Vec<SparklinePoint> = sorted
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let t = if point_count > 1 {
                    i as f32 / (point_count - 1) as f32
                } else {
                    0.0
                };
                let sgv_mgdl = e.sgv as f64;
                let status = if sgv_mgdl < target_low {
                    GlucoseStatus::Low
                } else if sgv_mgdl > target_high {
                    GlucoseStatus::High
                } else {
                    GlucoseStatus::InRange
                };
                let sgv = if is_mmol {
                    e.sgv as f32 / 18.0
                } else {
                    e.sgv as f32
                };
                SparklinePoint { t, sgv, status }
            })
            .collect();

        let display_sgv = if is_mmol {
            entry.sgv as f32 / 18.0
        } else {
            entry.sgv as f32
        };
        let display_delta = if is_mmol { delta / 18.0 } else { delta };
        let (unit_str, delta_str) = if is_mmol {
            (
                "mmol/L".to_string(),
                format!("{:+.1} mmol/L", display_delta),
            )
        } else {
            ("mg/dL".to_string(), format!("{:+.0} mg/dL", display_delta))
        };

        let current_rate = sorted.windows(2).last().and_then(|w| {
            let dt_min = (w[1].date - w[0].date) as f32 / 60_000.0;
            (dt_min > 0.0).then_some((w[1].sgv as f32 - w[0].sgv as f32) / dt_min)
        });

        let info_pill = pick_info_pill(
            entry.sgv as f32,
            current_status,
            current_rate,
            duration.num_minutes(),
        );

        let data = BgCardData {
            current_sgv: display_sgv,
            status: current_status,
            trend_arrow: entry.direction.as_arrow().to_string(),
            delta_str,
            age_str,
            unit_str,
            time_str: now.format("%H:%M").to_string(),
            watermark_str: "Beetroot".to_string(),
            iob_str,
            cob_str,
            sparkline_points,
            info_pill,
        };

        let db = &ctx.data().database;
        let theme = theme_assets::resolve_user_theme(
            db,
            target_id.get(),
            user_data.active_theme.as_deref(),
        )
        .await;
        let user_stickers = db.get_all_user_stickers(target_id.get()).await?;
        let bonbon_stickers = sticker_assets::load_bonbon_stickers(&user_stickers).await;

        let img_buffer = tokio::task::spawn_blocking(move || {
            let mut builder = BgCardBuilder::new()
                .with_data(data)
                .with_theme(theme)
                .with_scale(4.0);

            if !bonbon_stickers.is_empty() {
                let mut set =
                    StickerSet::new(bonbon_stickers.len().min(6)).with_stickers(bonbon_stickers);
                if let Some(rate) = current_rate {
                    set = set.with_current_rate(rate);
                }
                builder = builder.with_stickers(set);
            }

            let img = builder
                .build()
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;

            let mut buffer = Vec::with_capacity(100_000);
            PngEncoder::new_with_quality(
                &mut buffer,
                CompressionType::Level(6),
                FilterType::NoFilter,
            )
            .write_image(
                img.as_raw(),
                img.width(),
                img.height(),
                ExtendedColorType::Rgba8,
            )?;
            Ok::<Vec<u8>, anyhow::Error>(buffer)
        })
        .await??;

        ctx.send(
            poise::CreateReply::default().attachment(CreateAttachment::bytes(img_buffer, "bg.png")),
        )
        .await?;

        return Ok(());
    }

    let entries = if let Some(ago) = lookback {
        let target_time = now - ago;
        let window = chrono::Duration::minutes(10);

        let result = client
            .sgv()
            .get()
            .from(target_time - window)
            .to(target_time + window)
            .limit(50)
            .send()
            .await;

        match result {
            Ok(mut e) if !e.is_empty() => {
                e.sort_by_key(|entry| {
                    let entry_time =
                        chrono::DateTime::from_timestamp_millis(entry.date).unwrap_or(now);
                    (entry_time - target_time).num_seconds().unsigned_abs()
                });
                e
            }
            _ => {
                send_error!(
                    ctx,
                    "No Data",
                    format!(
                        "No glucose data found around **{}** ago.",
                        at_str.as_deref().unwrap_or("?")
                    )
                );
                return Ok(());
            }
        }
    } else {
        let entries_builder = client.sgv();
        match entries_builder.get().limit(2).send().await {
            Ok(e) if !e.is_empty() => e,
            _ => {
                send_error!(
                    ctx,
                    "Fetch Error",
                    "Could not fetch blood glucose data. Please check your URL."
                );
                return Ok(());
            }
        }
    };

    let properties_builder = client
        .properties()
        .get()
        .only(&[PropertyType::Iob, PropertyType::Cob]);
    let properties_fut = properties_builder.send();

    let profiles_builder = client.profiles();
    let profile_fut = profiles_builder.get();

    // cinnamon's Status struct expects numeric fields that some NS instances return as strings;
    // fetch raw JSON and extract only the custom title to avoid the deserialization bug.
    let status_fut = {
        let client = client.clone();
        async move {
            let url = client.base_url.join("api/v2/status.json").ok()?;
            let req = client.auth(client.http.get(url));
            let val: serde_json::Value = req.send().await.ok()?.json().await.ok()?;
            val.pointer("/settings/customTitle")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        }
    };

    let (properties_result, profile_result, custom_title) =
        tokio::join!(properties_fut, profile_fut, status_fut);

    tracing::debug!(entries = entries.len(), "bg data fetched");
    crate::log_medical!(
        entries = ?entries,
        properties = ?properties_result,
        profiles = ?profile_result,
        custom_title = ?custom_title,
        "bg raw data dump"
    );

    let entry = &entries[0];
    let prev_entry = entries.get(1);

    let delta = if let Some(prev) = prev_entry {
        entry.sgv as f64 - prev.sgv as f64
    } else {
        0.0
    };

    let (target_low, target_high) = if let Ok(profiles) = profile_result {
        if let Some(profile) = profiles.first() {
            let default_name = &profile.default_profile_name;
            if let Some(store) = profile.store.get(default_name) {
                let low = store.target_low.first().map(|x| x.value).unwrap_or(4.0);
                let high = store.target_high.first().map(|x| x.value).unwrap_or(10.0);
                let is_mmol = store.units.starts_with("mmol");
                if is_mmol {
                    (low * 18.0, high * 18.0)
                } else {
                    (low, high)
                }
            } else {
                (72.0, 180.0)
            }
        } else {
            (72.0, 180.0)
        }
    } else {
        (72.0, 180.0)
    };

    let entry_time = chrono::DateTime::parse_from_rfc3339(&entry.date_string)
        .unwrap_or_else(|_| chrono::Utc::now().into())
        .with_timezone(&chrono::Utc);

    let duration = now.signed_duration_since(entry_time);

    let time_ago = if duration.num_minutes() < 60 {
        format!("{} minutes ago", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{} hours ago", duration.num_hours())
    } else {
        format!("{} days ago", duration.num_days())
    };

    let color = if (entry.sgv as f64) > target_high {
        Colour::from_rgb(227, 177, 11)
    } else if (entry.sgv as f64) < target_low {
        Colour::from_rgb(235, 47, 47)
    } else {
        Colour::from_rgb(87, 189, 79)
    };

    let title = custom_title.unwrap_or_else(|| format!("{}'s Nightscout", target_user.name));

    let thumbnail_url = target_user.avatar_url().unwrap_or_default();

    let icon_bytes = tokio::fs::read("assets/images/nightscout_icon.png").await?;
    let icon_attachment = CreateAttachment::bytes(icon_bytes, "nightscout_icon.png");

    let mut embed = CreateEmbed::new()
        .thumbnail(thumbnail_url)
        .title(title)
        .color(color);

    let is_data_old = duration.num_minutes() > 15;
    if is_data_old {
        embed = embed.field(
            format!("{} Warning {}", emojis::DATE_INVALID, emojis::DATE_INVALID),
            format!("Data is {}min old!", duration.num_minutes()),
            false,
        );
    }

    let sgv_val = entry.sgv;
    let mmol_val = entry.sgv as f64 / 18.0;
    let delta_mmol = delta / 18.0;

    let delta_str = format!("{:+}", delta);
    let delta_mmol_str = format!("{:.1}", delta_mmol);
    let delta_mmol_formatted = if delta > 0.0 {
        format!("+{}", delta_mmol_str)
    } else {
        delta_mmol_str
    };

    let (mgdl_field, mmol_field) = if is_data_old {
        (
            format!("~~{} ({})~~", sgv_val, delta_str),
            format!("~~{:.1} ({})~~", mmol_val, delta_mmol_formatted),
        )
    } else {
        (
            format!("{} ({})", sgv_val, delta_str),
            format!("{:.1} ({})", mmol_val, delta_mmol_formatted),
        )
    };

    embed = embed
        .field("mg/dL", mgdl_field, true)
        .field("mmol/L", mmol_field, true)
        .field("Trend", entry.direction.as_arrow(), true);

    if let Ok(props) = properties_result {
        if let Some(iob) = props.iob
            && iob.iob > 0.0
        {
            embed = embed.field("IOB", format!("{:.2}u", iob.iob), true);
        }
        if let Some(cob) = props.cob
            && cob.cob > 0.0
        {
            embed = embed.field("COB", format!("{:.0}g", cob.cob), true);
        }
    }

    if lookback.is_none() {
        let expiry_mins = user_data.mbg_expiry_time;
        let since = now - chrono::Duration::minutes(expiry_mins);

        let (mbg_res, bgcheck_res) = tokio::join!(
            client.mbg().get().limit(1).send(),
            client.treatments().get().from(since).limit(10).send()
        );

        crate::log_medical!(mbg = ?mbg_res, bgcheck = ?bgcheck_res, "bg fingerprick lookup");

        let from_mbg = mbg_res.ok().and_then(|list| {
            list.into_iter().next().and_then(|mbg| {
                let t = chrono::DateTime::parse_from_rfc3339(&mbg.date_string).ok()?;
                let age = now
                    .signed_duration_since(t.with_timezone(&chrono::Utc))
                    .num_minutes();
                if age <= expiry_mins {
                    Some((mbg.mbg as f64, age))
                } else {
                    None
                }
            })
        });

        let from_bgcheck = bgcheck_res.ok().and_then(|treatments| {
            treatments
                .into_iter()
                .filter(|t| t.event_type == "BG Check")
                .filter_map(|t| {
                    let glucose = t.glucose?;
                    let dt = chrono::DateTime::parse_from_rfc3339(&t.created_at).ok()?;
                    let age = now
                        .signed_duration_since(dt.with_timezone(&chrono::Utc))
                        .num_minutes();
                    if age <= expiry_mins {
                        Some((glucose, age))
                    } else {
                        None
                    }
                })
                .min_by_key(|(_, age)| *age)
        });

        let fingerprick = [from_mbg, from_bgcheck]
            .into_iter()
            .flatten()
            .min_by_key(|(_, age)| *age);

        if let Some((val, age)) = fingerprick {
            let val_mmol = val / 18.0;
            embed = embed.field(
                "Fingerprick",
                format!(
                    "{:.0} mg/dL ({:.1} mmol/L)\n-# {} min ago",
                    val, val_mmol, age
                ),
                false,
            );
        }
    }

    embed = embed.footer(
        CreateEmbedFooter::new(format!("measured • {time_ago}"))
            .icon_url("attachment://nightscout_icon.png"),
    );

    ctx.send(
        poise::CreateReply::default()
            .embed(embed)
            .attachment(icon_attachment),
    )
    .await?;

    Ok(())
}

fn pick_info_pill(
    sgv_mgdl: f32,
    status: GlucoseStatus,
    rate: Option<f32>,
    age_min: i64,
) -> Option<InfoPill> {
    use bonbon::prelude::builtin_icons;

    let make = |icon: &'static [u8], text: &str, state: PillState| InfoPill {
        icon: PillIcon::from_bytes(icon.to_vec()),
        text: text.to_string(),
        state,
    };

    if sgv_mgdl < 55.0 {
        return Some(make(
            builtin_icons::WARNING,
            "Treat now",
            PillState::AlertLow,
        ));
    }
    if sgv_mgdl > 250.0 {
        return Some(make(
            builtin_icons::WARNING,
            "Very high",
            PillState::AlertHigh,
        ));
    }

    if age_min > 30 {
        return Some(make(
            builtin_icons::FINGERPRICK,
            "Fingerprick",
            PillState::AlertHigh,
        ));
    }
    if age_min > 15 {
        return Some(make(
            builtin_icons::WARNING,
            "Stale data",
            PillState::Normal,
        ));
    }

    const FAST_THRESHOLD: f32 = 2.0;
    if let Some(r) = rate
        && r.abs() >= FAST_THRESHOLD
    {
        let rising = r > 0.0;
        return Some(match (status, rising) {
            (GlucoseStatus::Low, false) => make(
                builtin_icons::FAST_DROP,
                "Falling fast",
                PillState::AlertLow,
            ),
            (GlucoseStatus::High, true) => make(
                builtin_icons::FAST_RISE,
                "Rising fast",
                PillState::AlertHigh,
            ),
            (GlucoseStatus::Low, true) => {
                make(builtin_icons::FAST_RISE, "Recovering", PillState::Normal)
            }
            (GlucoseStatus::High, false) => {
                make(builtin_icons::FAST_DROP, "Coming down", PillState::Normal)
            }
            (GlucoseStatus::InRange, true) => {
                make(builtin_icons::FAST_RISE, "Rising", PillState::AlertHigh)
            }
            (GlucoseStatus::InRange, false) => {
                make(builtin_icons::FAST_DROP, "Falling", PillState::AlertLow)
            }
        });
    }

    match status {
        GlucoseStatus::Low => Some(make(builtin_icons::WARNING, "Low", PillState::AlertLow)),
        GlucoseStatus::High => Some(make(builtin_icons::WARNING, "High", PillState::AlertHigh)),
        GlucoseStatus::InRange => None,
    }
}
