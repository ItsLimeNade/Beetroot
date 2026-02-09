use crate::data::{Context, Error};
use crate::{check_privacy, fetch_graph_data, get_db_user, get_nightscout_client};
use bonbon::prelude::*;
use chrono::{Duration, Utc};
use image::ImageEncoder;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use serenity::all::CreateAttachment;

#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("graph")]
pub async fn graph(
    ctx: Context<'_>,
    #[description = "Hours of data to display (3-24)"]
    #[min = 3]
    #[max = 24]
    hours: Option<i64>,
    #[description = "View another user's graph"] user: Option<serenity::User>,
) -> Result<(), Error> {
    let target_user = user.as_ref().unwrap_or(ctx.author());
    let target_id = target_user.id;

    let user_data = get_db_user!(ctx, target_id.get());

    check_privacy!(ctx, target_id, user_data);

    let client = get_nightscout_client!(ctx, user_data);

    ctx.defer().await?;

    let duration_hours = hours.unwrap_or(3);
    let now = Utc::now();
    let start_time = now - Duration::hours(duration_hours) - Duration::minutes(15);

    let (entries, treatments, profiles) = fetch_graph_data!(ctx, client, start_time, now);

    if entries.is_empty() {
        send_error!(
            ctx,
            "No Data",
            "No glucose entries found for the specified time range."
        );
        return Ok(());
    }

    let (target_low, target_high) = profiles
        .as_ref()
        .and_then(|p| p.first())
        .and_then(|p| p.store.get(&p.default_profile_name))
        .map(|store| {
            let low = store.target_low.first().map(|x| x.value).unwrap_or(80.0);
            let high = store.target_high.first().map(|x| x.value).unwrap_or(180.0);
            (low as f32, high as f32)
        })
        .unwrap_or((80.0, 180.0));

    let graph_image = tokio::task::spawn_blocking(move || {
        let layout = LayoutConfig {
            width: 2550,
            height: 1650,
            ..Default::default()
        };

        GlucoseGraphBuilder::new()
            .with_treatment_mode(TreatmentDisplayMode::Contextual)
            .with_scaling(GraphScaling::Dynamic {
                clamp_min: 40.0,
                clamp_max: 400.0,
                default_min: 60.0,
                default_max: 200.0,
            })
            .with_layout(layout)
            .with_theme(Theme::dark())
            .with_units(UnitDisplay::Dual {
                primary: UnitPreference::MgDl,
            })
            .with_targets(target_low, target_high)
            .add_entries(entries)
            .add_treatments(treatments)
            .with_time_axis(TimeAxisMode::EquallyDistributed { count: 6 })
            .with_fixed_duration(Duration::hours(duration_hours))
            .build()
            .map_err(|e| anyhow::anyhow!(e.to_string()))
    })
    .await??;

    let img_buffer = tokio::task::spawn_blocking(move || {
        let mut buffer = Vec::with_capacity(200_000);
        let encoder = image::codecs::png::PngEncoder::new_with_quality(
            &mut buffer,
            image::codecs::png::CompressionType::Level(9),
            image::codecs::png::FilterType::NoFilter,
        );

        encoder.write_image(
            &graph_image,
            graph_image.width(),
            graph_image.height(),
            image::ExtendedColorType::Rgba8,
        )?;
        Ok::<Vec<u8>, anyhow::Error>(buffer)
    })
    .await??;

    let attachment = CreateAttachment::bytes(img_buffer, "graph.png");

    ctx.send(poise::CreateReply::default().attachment(attachment))
        .await?;

    Ok(())
}
