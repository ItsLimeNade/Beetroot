use crate::data::{Context, Error};
use crate::utils::duration_parser::parse_ago_duration;
use crate::utils::emojis;
use crate::utils::render;
use crate::utils::targets::resolve_profile_targets_mgdl;
use crate::utils::theme_assets;
use bonbon::prelude::*;
use cinnamon::model::properties::Property;
use cinnamon::model::{Direction, EventType, Glucose, Units};
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

    // Honor the data owner's force_ephemeral preference (see graph.rs).
    let reply_ephemeral = user_data.force_ephemeral;

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

    crate::tips::safe_defer_with(ctx, reply_ephemeral).await?;

    let now = chrono::Utc::now();

    if user_data.bg_image_mode {
        let sparkline_entries = if let Some(ago) = lookback {
            let target_time = now - ago;
            let window_start = target_time - chrono::Duration::hours(3);
            match client
                .entries()
                .sgv()
                .list()
                .since(window_start)
                .until(target_time + chrono::Duration::minutes(5))
                .limit(50)
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
            match client.entries().sgv().list().limit(36).await {
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
            .only([Property::Iob, Property::Cob])
            .send();
        let profiles = client.profiles();
        let profile_fut = profiles.current();
        let server = client.server();
        let status_fut = server.status();

        let (properties_result, profile_result, status_result) =
            tokio::join!(properties_fut, profile_fut, status_fut);
        let custom_title = custom_title(status_result);

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

        let (target_low, target_high, is_mmol) = profile_result
            .as_ref()
            .ok()
            .and_then(|profile| profile.as_ref()?.default_entry())
            .map(resolve_profile_targets_mgdl)
            .unwrap_or((72.0, 180.0, false));

        let mut sorted = sparkline_entries;
        sorted.sort_by_key(|e| e.date);

        let point_count = sorted.len();
        let entry = sorted.last().unwrap();
        let prev = if point_count >= 2 {
            sorted.get(point_count - 2)
        } else {
            None
        };
        let sgv_mgdl = entry.sgv.as_mgdl() as f32;
        let delta = prev
            .map(|p| entry.sgv.as_mgdl() - p.sgv.as_mgdl())
            .unwrap_or(0.0);

        let duration = now.signed_duration_since(entry.date.to_datetime());
        let age_str = if duration.num_minutes() < 60 {
            format!("{} min ago", duration.num_minutes())
        } else {
            format!("{} h ago", duration.num_hours())
        };

        let current_status = if sgv_mgdl < target_low {
            GlucoseStatus::Low
        } else if sgv_mgdl > target_high {
            GlucoseStatus::High
        } else {
            GlucoseStatus::InRange
        };

        let iob_str = properties_result.as_ref().ok().and_then(|props| {
            props
                .iob
                .as_ref()
                .and_then(|i| i.iob)
                .filter(|iob| *iob > 0.0)
                .map(|iob| format!("IOB {:.2}u", iob))
        });
        let cob_str = properties_result.as_ref().ok().and_then(|props| {
            props
                .cob
                .as_ref()
                .and_then(|c| c.cob)
                .filter(|cob| *cob > 0.0)
                .map(|cob| format!("COB {:.0}g", cob))
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
                let sgv_mgdl = e.sgv.as_mgdl() as f32;
                let status = if sgv_mgdl < target_low {
                    GlucoseStatus::Low
                } else if sgv_mgdl > target_high {
                    GlucoseStatus::High
                } else {
                    GlucoseStatus::InRange
                };
                SparklinePoint {
                    t,
                    sgv: sgv_mgdl,
                    status,
                }
            })
            .collect();

        let current_rate = sorted.windows(2).last().and_then(|w| {
            let dt_min = (w[1].date.as_millis() - w[0].date.as_millis()) as f32 / 60_000.0;
            let rise = (w[1].sgv.as_mgdl() - w[0].sgv.as_mgdl()) as f32;
            (dt_min > 0.0).then_some(rise / dt_min)
        });

        let info_pill = pick_info_pill(
            sgv_mgdl,
            current_status,
            current_rate,
            duration.num_minutes(),
        );

        let data = BgCardData {
            current_sgv: sgv_mgdl,
            status: current_status,
            trend_arrow: trend_arrow(entry.direction.as_ref()).to_string(),
            delta: Some(delta as f32),
            age_str,
            time_str: now.format("%H:%M").to_string(),
            watermark_str: custom_title
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_else(|| format!("{}'s Nightscout", target_user.name)),
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
        let img_buffer = render::run_blocking(move || {
            let builder = BgCardBuilder::new()
                .with_data(data)
                .with_units(UnitDisplay::Dual {
                    primary: if is_mmol {
                        UnitPreference::MmolL
                    } else {
                        UnitPreference::MgDl
                    },
                })
                .with_theme(theme)
                .with_scale(4.0);

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
        .await?;

        ctx.send(
            poise::CreateReply::default()
                .attachment(CreateAttachment::bytes(img_buffer, "bg.png"))
                .ephemeral(reply_ephemeral),
        )
        .await?;

        return Ok(());
    }

    let entries = if let Some(ago) = lookback {
        let target_time = now - ago;
        let window = chrono::Duration::minutes(10);

        let result = client
            .entries()
            .sgv()
            .list()
            .since(target_time - window)
            .until(target_time + window)
            .limit(50)
            .await;

        match result {
            Ok(mut e) if !e.is_empty() => {
                e.sort_by_key(|entry| {
                    (entry.date.to_datetime() - target_time)
                        .num_seconds()
                        .unsigned_abs()
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
        match client.entries().sgv().list().limit(2).await {
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
        .only([Property::Iob, Property::Cob])
        .send();

    let profiles = client.profiles();
    let profile_fut = profiles.current();

    let server = client.server();
    let status_fut = server.status();

    let (properties_result, profile_result, status_result) =
        tokio::join!(properties_fut, profile_fut, status_fut);
    let custom_title = custom_title(status_result);

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

    let sgv_mgdl = entry.sgv.as_mgdl();
    let delta = if let Some(prev) = prev_entry {
        sgv_mgdl - prev.sgv.as_mgdl()
    } else {
        0.0
    };

    let default_profile = profile_result
        .as_ref()
        .ok()
        .and_then(|profile| profile.as_ref()?.default_entry());
    let (target_low, target_high) = if let Some(store) = default_profile {
        let low = store.target_low.first().map(|x| x.value).unwrap_or(4.0);
        let high = store.target_high.first().map(|x| x.value).unwrap_or(10.0);
        let is_mmol = store.units() == Some(Units::MmolL);
        if is_mmol {
            (low * 18.0, high * 18.0)
        } else {
            (low, high)
        }
    } else {
        (72.0, 180.0)
    };

    let duration = now.signed_duration_since(entry.date.to_datetime());

    let time_ago = if duration.num_minutes() < 60 {
        format!("{} minutes ago", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{} hours ago", duration.num_hours())
    } else {
        format!("{} days ago", duration.num_days())
    };

    let color = if sgv_mgdl > target_high {
        Colour::from_rgb(227, 177, 11)
    } else if sgv_mgdl < target_low {
        Colour::from_rgb(235, 47, 47)
    } else {
        Colour::from_rgb(87, 189, 79)
    };

    let title = custom_title.unwrap_or_else(|| format!("{}'s Nightscout", target_user.name));

    let icon_bytes = tokio::fs::read("assets/images/nightscout_icon.png").await?;
    let icon_attachment = CreateAttachment::bytes(icon_bytes, "nightscout_icon.png");

    let mut embed = CreateEmbed::new().title(title).color(color);
    if let Some(avatar_url) = target_user.avatar_url() {
        embed = embed.thumbnail(avatar_url);
    }

    let is_data_old = duration.num_minutes() > 15;
    if is_data_old {
        embed = embed.field(
            format!(
                "{} Warning {}",
                emojis::date_invalid(),
                emojis::date_invalid()
            ),
            format!("Data is {}min old!", duration.num_minutes()),
            false,
        );
    }

    let sgv_val = sgv_mgdl;
    let mmol_val = sgv_mgdl / 18.0;
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
        .field("Trend", trend_arrow(entry.direction.as_ref()), true);

    if let Ok(props) = properties_result {
        if let Some(iob) = props.iob.and_then(|i| i.iob)
            && iob > 0.0
        {
            embed = embed.field(
                format!("{} IOB", emojis::micro_bolus()),
                format!("{:.2}u", iob),
                true,
            );
        }
        if let Some(cob) = props.cob.and_then(|c| c.cob)
            && cob > 0.0
        {
            embed = embed.field(
                format!("{} COB", emojis::carbs()),
                format!("{:.0}g", cob),
                true,
            );
        }
    }

    if lookback.is_none() {
        let expiry_mins = user_data.mbg_expiry_time;
        let since = now - chrono::Duration::minutes(expiry_mins);

        let mbg = client.entries().mbg();
        let (mbg_res, bgcheck_res) = tokio::join!(
            mbg.latest(),
            client.treatments().list().since(since).limit(10).send()
        );

        crate::log_medical!(mbg = ?mbg_res, bgcheck = ?bgcheck_res, "bg fingerprick lookup");

        let from_mbg = mbg_res.ok().flatten().and_then(|mbg| {
            let age = now
                .signed_duration_since(mbg.date.to_datetime())
                .num_minutes();
            if age <= expiry_mins {
                Some((mbg.mbg.as_mgdl(), age))
            } else {
                None
            }
        });

        let from_bgcheck = bgcheck_res.ok().and_then(|treatments| {
            treatments
                .into_iter()
                .filter(|t| t.event_type == Some(EventType::BgCheck))
                .filter_map(|t| {
                    // `glucose` is stored in the treatment's own units.
                    let units = t.units.as_deref().and_then(Units::parse);
                    let glucose = Glucose::new(t.glucose?, units.unwrap_or_default()).as_mgdl();
                    let age = now
                        .signed_duration_since(t.time()?.to_datetime())
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
            .attachment(icon_attachment)
            .ephemeral(reply_ephemeral),
    )
    .await?;

    Ok(())
}

/// The site's custom title from its status, if it could be fetched.
fn custom_title(status: cinnamon::Result<cinnamon::model::system::Status>) -> Option<String> {
    status.ok()?.settings?.custom_title
}

/// Arrow shown for a trend. Sticks to glyphs the card font is known to render.
fn trend_arrow(direction: Option<&Direction>) -> &'static str {
    match direction {
        Some(Direction::TripleUp) => "↑↑↑",
        Some(Direction::DoubleUp) => "↑↑",
        Some(Direction::SingleUp) => "↑",
        Some(Direction::FortyFiveUp) => "↗",
        Some(Direction::Flat) => "→",
        Some(Direction::FortyFiveDown) => "↘",
        Some(Direction::SingleDown) => "↓",
        Some(Direction::DoubleDown) => "↓↓",
        Some(Direction::TripleDown) => "↓↓↓",
        _ => "↮",
    }
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
            "Severe low, verify reading",
            PillState::AlertLow,
        ));
    }
    if sgv_mgdl > 250.0 {
        return Some(make(
            builtin_icons::WARNING,
            "Very high, verify reading",
            PillState::AlertHigh,
        ));
    }

    if age_min > 30 {
        return Some(make(
            builtin_icons::FINGERPRICK,
            "Confirm with fingerstick",
            PillState::AlertHigh,
        ));
    }
    if age_min > 15 {
        return Some(make(
            builtin_icons::WARNING,
            "Reading may be outdated",
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
                "Dropping fast, monitor",
                PillState::AlertLow,
            ),
            (GlucoseStatus::High, true) => make(
                builtin_icons::FAST_RISE,
                "Rising fast, monitor",
                PillState::AlertHigh,
            ),
            (GlucoseStatus::Low, true) => make(
                builtin_icons::FAST_RISE,
                "Recovering, keep watch",
                PillState::Normal,
            ),
            (GlucoseStatus::High, false) => make(
                builtin_icons::FAST_DROP,
                "Coming down, keep watch",
                PillState::Normal,
            ),
            (GlucoseStatus::InRange, true) => make(
                builtin_icons::FAST_RISE,
                "Trending up, monitor",
                PillState::AlertHigh,
            ),
            (GlucoseStatus::InRange, false) => make(
                builtin_icons::FAST_DROP,
                "Trending down, watch lows",
                PillState::AlertLow,
            ),
        });
    }

    match status {
        GlucoseStatus::Low => Some(make(
            builtin_icons::WARNING,
            "Low, confirm reading",
            PillState::AlertLow,
        )),
        GlucoseStatus::High => Some(make(
            builtin_icons::WARNING,
            "High, keep monitoring",
            PillState::AlertHigh,
        )),
        GlucoseStatus::InRange => None,
    }
}
