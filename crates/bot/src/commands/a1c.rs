use crate::data::{Context, Error};
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateAttachment, CreateEmbed, CreateEmbedFooter};
use tracing::{debug, warn};

/// Gives your estimated A1C over the past 3 months.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("a1c")]
pub async fn a1c(ctx: Context<'_>) -> Result<(), Error> {
    let user_id = ctx.author().id.get();
    debug!("[a1c] invoked by user_id={}", user_id);

    let user_data = get_db_user!(ctx, user_id);
    let client = get_nightscout_client!(ctx, user_data);

    ctx.defer().await?;

    let now = chrono::Utc::now();
    let lookback = chrono::Months::new(3);
    let ago = now - lookback;
    debug!("[a1c] querying SGV from={} to={}", ago, now);

    let result = client.sgv().get().limit(120_000).from(ago).send().await;

    let icon_bytes = std::fs::read("assets/images/nightscout_icon.png")?;
    let icon_attachment = CreateAttachment::bytes(icon_bytes, "nightscout_icon.png");

    let mut embed = CreateEmbed::new()
        .title("Estimated A1C")
        .description(
            "Based on your average glucose over the past 3 months.\n\
             *This does not replace an actual A1C blood test.*",
        )
        .footer(
            CreateEmbedFooter::new("Estimation only • Not a substitute for lab results")
                .icon_url("attachment://nightscout_icon.png"),
        );

    match result {
        Ok(entries) if !entries.is_empty() => {
            debug!("[a1c] received {} SGV entries", entries.len());
            debug!(
                "[a1c] newest entry date_string={:?}",
                entries.first().map(|e| &e.date_string)
            );
            debug!(
                "[a1c] oldest entry date_string={:?}",
                entries.last().map(|e| &e.date_string)
            );

            let tolerance = chrono::Duration::days(5);
            let oldest_date_string = &entries.last().unwrap().date_string;
            let oldest_date_res = chrono::DateTime::parse_from_rfc3339(oldest_date_string);

            match oldest_date_res {
                Ok(oldest_date) => {
                    let oldest_utc = oldest_date.to_utc();
                    let data_gap = oldest_utc - ago;

                    debug!(
                        "[a1c] oldest_date={} ago={} data_gap={:.2}days tolerance={:.2}days insufficient_data={}",
                        oldest_utc,
                        ago,
                        data_gap.num_minutes() as f64 / 1440.0,
                        tolerance.num_minutes() as f64 / 1440.0,
                        data_gap > tolerance,
                    );

                    let has_warning = data_gap > tolerance;
                    if has_warning {
                        warn!(
                            "[a1c] insufficient data coverage ({:.1} days missing) — adding warning field",
                            data_gap.num_minutes() as f64 / 1440.0
                        );
                        embed = embed.field(
                            "⚠️ Incomplete Data",
                            format!(
                                "Data only goes back {:.0} days instead of ~90. \
                                 This estimate may be less accurate.",
                                (now - oldest_utc).num_days()
                            ),
                            false,
                        );
                    }

                    let eag = calc_eag(&entries);
                    let a1c = calc_a1c(eag);

                    debug!(
                        "[a1c] entry_count={} eAG={:.1} a1c={:.1}% has_warning={}",
                        entries.len(),
                        eag,
                        a1c,
                        has_warning,
                    );

                    let color = if has_warning {
                        Colour::from_rgb(235, 47, 47) // red - incomplete data
                    } else {
                        Colour::from_rgb(87, 189, 79) // green - full coverage
                    };

                    embed = embed
                        .color(color)
                        .field(
                            "Data Range",
                            format!(
                                "<t:{}:D> → <t:{}:D>",
                                oldest_utc.timestamp(),
                                now.timestamp()
                            ),
                            false,
                        )
                        .field("Readings Used", format!("{}", entries.len()), true)
                        .field("A1C Estimation", format!("{:.1}%", a1c), true);
                }
                Err(e) => {
                    warn!(
                        "[a1c] failed to parse oldest entry date_string={:?} error={}",
                        oldest_date_string, e
                    );
                    send_error!(
                        ctx,
                        "Error Parsing Time",
                        "There was an error while parsing the time for the last entry."
                    );
                    return Ok(());
                }
            }
        }
        Ok(_) => {
            debug!("[a1c] query returned 0 entries (empty Vec)");
            send_error!(ctx, "No Data", "No glucose data found.");
            return Ok(());
        }
        Err(e) => {
            warn!("[a1c] nightscout SGV request failed: {}", e);
            send_error!(ctx, "No Data", "No glucose data found.");
            return Ok(());
        }
    }

    debug!("[a1c] sending embed");
    ctx.send(
        poise::CreateReply::default()
            .embed(embed)
            .attachment(icon_attachment),
    )
    .await?;

    Ok(())
}

/// Calculates the estimated average glucose of the given dataset.
///
/// Returns f64 to avoid integer truncation in the A1C formula.
fn calc_eag(entries: &[cinnamon::models::entries::SgvEntry]) -> f64 {
    // Bug fix #5: sum as f64, divide as f64
    entries.iter().map(|s| s.sgv as f64).sum::<f64>() / entries.len() as f64
}

/// Calculates estimated A1C from estimated average glucose (eAG).
///
/// Formula: eAG = 28.7 * A1C - 46.7  ->  A1C = (eAG + 46.7) / 28.7
///
/// Truncated to one decimal place.
fn calc_a1c(eag: f64) -> f64 {
    ((eag + 46.7) / 28.7 * 10.0).floor() / 10.0
}
