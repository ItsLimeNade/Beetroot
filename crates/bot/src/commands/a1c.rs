use crate::data::{Context, Error};
use crate::utils::emojis;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::{Colour, CreateAttachment, CreateEmbed, CreateEmbedFooter};
use tracing::{debug, warn};

/// Gives your estimated A1C over the past 3 months.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel",
    user_cooldown = 10
)]
#[track_analytics("a1c")]
pub async fn a1c(ctx: Context<'_>) -> Result<(), Error> {
    let user_id = ctx.author().id.get();
    debug!(user = %crate::logging::redact(user_id), "computing 3-month A1C estimate");

    let user_data = get_db_user!(ctx, user_id);
    let reply_ephemeral = user_data.force_ephemeral;
    let client = get_nightscout_client!(ctx, user_data);

    crate::tips::safe_defer_with(ctx, reply_ephemeral).await?;

    let now = chrono::Utc::now();
    let lookback = chrono::Months::new(3);
    let ago = now - lookback;
    debug!(from = %ago, to = %now, "querying SGV window");

    let result = client.sgv().get().limit(120_000).from(ago).send().await;

    let icon_bytes = tokio::fs::read("assets/images/nightscout_icon.png").await?;
    let icon_attachment = CreateAttachment::bytes(icon_bytes, "nightscout_icon.png");

    let mut embed = CreateEmbed::new()
        .title(format!("{} Estimated A1C", emojis::sugar()))
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
            debug!(
                count = entries.len(),
                newest = ?entries.first().and_then(|e| e.datetime()),
                oldest = ?entries.last().and_then(|e| e.datetime()),
                "received SGV entries"
            );

            let tolerance = chrono::Duration::days(5);
            let oldest_entry = entries.last().unwrap();

            match oldest_entry.datetime() {
                Some(oldest_utc) => {
                    let data_gap = oldest_utc - ago;

                    debug!(
                        data_gap_days = data_gap.num_minutes() as f64 / 1440.0,
                        tolerance_days = tolerance.num_minutes() as f64 / 1440.0,
                        insufficient = data_gap > tolerance,
                        "evaluated data coverage"
                    );

                    let has_warning = data_gap > tolerance;
                    if has_warning {
                        warn!(
                            missing_days = data_gap.num_minutes() as f64 / 1440.0,
                            "insufficient A1C data coverage, adding warning field"
                        );
                        embed = embed.field(
                            format!("{} Incomplete Data", emojis::date_invalid()),
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

                    debug!(count = entries.len(), has_warning, "computed A1C estimate");
                    // eAG and A1C are the user's average glucose: medical data.
                    crate::log_medical!(eag, a1c, "A1C estimate values");

                    let color = if has_warning {
                        Colour::from_rgb(235, 47, 47) // red - incomplete data
                    } else {
                        Colour::from_rgb(87, 189, 79) // green - full coverage
                    };

                    embed = embed
                        .color(color)
                        .field(
                            format!("{} Data Range", emojis::date_valid()),
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
                None => {
                    warn!(date = oldest_entry.date, "oldest entry has an out-of-range timestamp");
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
            debug!("query returned 0 entries");
            send_error!(ctx, "No Data", "No glucose data found.");
            return Ok(());
        }
        Err(e) => {
            warn!(error = %e, "nightscout SGV request failed");
            send_error!(ctx, "No Data", "No glucose data found.");
            return Ok(());
        }
    }

    debug!("sending A1C embed");
    ctx.send(
        poise::CreateReply::default()
            .embed(embed)
            .attachment(icon_attachment)
            .ephemeral(reply_ephemeral),
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
