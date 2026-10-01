use crate::data::{Context, Error};
use crate::utils::denoise::Strength;
use crate::utils::emojis;
use crate::utils::graph_render::{self, GraphWindow, ProfileSettings};
use crate::utils::search::{
    self, Extreme, Hit, Kind, MIN_EPISODE_MINUTES, Reading, Sort, find_episodes, sort_hits,
    treatment_hits,
};
use beetroot_core::models::UserDecrypted;
use bonbon::prelude::GraphTreatment;
use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use futures::StreamExt;
use macros::track_analytics;
use poise::serenity_prelude::{self as serenity, ComponentInteractionDataKind};
use serenity::{
    ButtonStyle, Colour, ComponentInteraction, CreateActionRow, CreateAttachment, CreateButton,
    CreateEmbed, CreateEmbedAuthor, CreateEmbedFooter, CreateInteractionResponse,
    CreateInteractionResponseFollowup, CreateInteractionResponseMessage, CreateSelectMenu,
    CreateSelectMenuKind, CreateSelectMenuOption, EditInteractionResponse,
};

/// Results listed per page.
const PAGE_SIZE: usize = 8;

/// How long the results stay interactive after the last click.
const IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// Most readings and treatments fetched for one search. Nightscout returns
/// the newest first, so hitting a limit drops the oldest part of the period.
const ENTRY_LIMIT: usize = 120_000;
const TREATMENT_LIMIT: usize = 20_000;

const GRAPH_FILENAME: &str = "graph.png";

const ID_VIEW: &str = "search_view";
const ID_SORT: &str = "search_sort";
const ID_PREV_PAGE: &str = "search_prev_page";
const ID_NEXT_PAGE: &str = "search_next_page";
const ID_PAGE_LABEL: &str = "search_page_label";
const ID_PREV_HIT: &str = "search_prev_hit";
const ID_NEXT_HIT: &str = "search_next_hit";
const ID_CLOSE: &str = "search_close";

/// How far back a search looks.
#[derive(Debug, Clone, Copy, poise::ChoiceParameter)]
pub enum SearchPeriod {
    #[name = "Last 24 hours"]
    Day,
    #[name = "Last 3 days"]
    ThreeDays,
    #[name = "Last 7 days"]
    Week,
    #[name = "Last 14 days"]
    Fortnight,
    #[name = "Last 30 days"]
    Month,
    #[name = "Last 90 days"]
    Quarter,
}

impl SearchPeriod {
    fn days(self) -> i64 {
        match self {
            Self::Day => 1,
            Self::ThreeDays => 3,
            Self::Week => 7,
            Self::Fortnight => 14,
            Self::Month => 30,
            Self::Quarter => 90,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Day => "Last 24 hours",
            Self::ThreeDays => "Last 3 days",
            Self::Week => "Last 7 days",
            Self::Fortnight => "Last 14 days",
            Self::Month => "Last 30 days",
            Self::Quarter => "Last 90 days",
        }
    }
}

#[derive(Debug, Clone, Copy, poise::ChoiceParameter)]
pub enum LowSort {
    #[name = "Most recent"]
    Recent,
    #[name = "Lowest"]
    Lowest,
    #[name = "Longest"]
    Longest,
    #[name = "Shortest"]
    Shortest,
    #[name = "Oldest"]
    Oldest,
}

#[derive(Debug, Clone, Copy, poise::ChoiceParameter)]
pub enum HighSort {
    #[name = "Most recent"]
    Recent,
    #[name = "Highest"]
    Highest,
    #[name = "Longest"]
    Longest,
    #[name = "Shortest"]
    Shortest,
    #[name = "Oldest"]
    Oldest,
}

#[derive(Debug, Clone, Copy, poise::ChoiceParameter)]
pub enum RangeSort {
    #[name = "Most recent"]
    Recent,
    #[name = "Longest"]
    Longest,
    #[name = "Shortest"]
    Shortest,
    #[name = "Oldest"]
    Oldest,
}

#[derive(Debug, Clone, Copy, poise::ChoiceParameter)]
pub enum AmountSort {
    #[name = "Most recent"]
    Recent,
    #[name = "Largest"]
    Largest,
    #[name = "Smallest"]
    Smallest,
    #[name = "Oldest"]
    Oldest,
}

/// What counts as low.
#[derive(Debug, Clone, Copy, poise::ChoiceParameter)]
pub enum LowLevel {
    #[name = "Below my target range"]
    Target,
    #[name = "Below 70 mg/dL (3.9 mmol/L)"]
    Low,
    #[name = "Below 54 mg/dL (3.0 mmol/L)"]
    VeryLow,
}

/// What counts as high.
#[derive(Debug, Clone, Copy, poise::ChoiceParameter)]
pub enum HighLevel {
    #[name = "Above my target range"]
    Target,
    #[name = "Above 180 mg/dL (10.0 mmol/L)"]
    High,
    #[name = "Above 250 mg/dL (13.9 mmol/L)"]
    VeryHigh,
}

/// Search your Nightscout history and open any result on a graph.
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel",
    subcommands("lows", "highs", "in_range", "carbs", "insulin")
)]
pub async fn search(_ctx: Context<'_>) -> Result<(), Error> {
    // Parent of a slash command group; never invoked directly.
    Ok(())
}

/// Find your lows: the lowest, the longest, the most recent...
#[poise::command(slash_command, user_cooldown = 10)]
#[track_analytics("search_lows")]
pub async fn lows(
    ctx: Context<'_>,
    #[description = "Which lows to list first (default: most recent)"] sort: Option<LowSort>,
    #[description = "How far back to search (default: last 7 days)"] period: Option<SearchPeriod>,
    #[description = "What counts as low (default: below your target range)"] level: Option<
        LowLevel,
    >,
    #[description = "Search another user's data"] user: Option<serenity::User>,
) -> Result<(), Error> {
    let sort = match sort.unwrap_or(LowSort::Recent) {
        LowSort::Recent => Sort::Recent,
        LowSort::Lowest => Sort::Lowest,
        LowSort::Longest => Sort::Longest,
        LowSort::Shortest => Sort::Shortest,
        LowSort::Oldest => Sort::Oldest,
    };
    let threshold = match level.unwrap_or(LowLevel::Target) {
        LowLevel::Target => None,
        LowLevel::Low => Some(70.0),
        LowLevel::VeryLow => Some(54.0),
    };
    run(
        ctx,
        Request {
            kind: Kind::Lows,
            sort,
            period: period.unwrap_or(SearchPeriod::Week),
            threshold,
            min_amount: None,
            user,
        },
    )
    .await
}

/// Find your highs: the highest, the longest, the most recent...
#[poise::command(slash_command, user_cooldown = 10)]
#[track_analytics("search_highs")]
pub async fn highs(
    ctx: Context<'_>,
    #[description = "Which highs to list first (default: most recent)"] sort: Option<HighSort>,
    #[description = "How far back to search (default: last 7 days)"] period: Option<SearchPeriod>,
    #[description = "What counts as high (default: above your target range)"] level: Option<
        HighLevel,
    >,
    #[description = "Search another user's data"] user: Option<serenity::User>,
) -> Result<(), Error> {
    let sort = match sort.unwrap_or(HighSort::Recent) {
        HighSort::Recent => Sort::Recent,
        HighSort::Highest => Sort::Highest,
        HighSort::Longest => Sort::Longest,
        HighSort::Shortest => Sort::Shortest,
        HighSort::Oldest => Sort::Oldest,
    };
    let threshold = match level.unwrap_or(HighLevel::Target) {
        HighLevel::Target => None,
        HighLevel::High => Some(180.0),
        HighLevel::VeryHigh => Some(250.0),
    };
    run(
        ctx,
        Request {
            kind: Kind::Highs,
            sort,
            period: period.unwrap_or(SearchPeriod::Week),
            threshold,
            min_amount: None,
            user,
        },
    )
    .await
}

/// Find your stretches in range: the longest, the most recent...
#[poise::command(slash_command, rename = "in-range", user_cooldown = 10)]
#[track_analytics("search_in_range")]
pub async fn in_range(
    ctx: Context<'_>,
    #[description = "Which stretches to list first (default: most recent)"] sort: Option<RangeSort>,
    #[description = "How far back to search (default: last 7 days)"] period: Option<SearchPeriod>,
    #[description = "Search another user's data"] user: Option<serenity::User>,
) -> Result<(), Error> {
    let sort = match sort.unwrap_or(RangeSort::Recent) {
        RangeSort::Recent => Sort::Recent,
        RangeSort::Longest => Sort::Longest,
        RangeSort::Shortest => Sort::Shortest,
        RangeSort::Oldest => Sort::Oldest,
    };
    run(
        ctx,
        Request {
            kind: Kind::InRange,
            sort,
            period: period.unwrap_or(SearchPeriod::Week),
            threshold: None,
            min_amount: None,
            user,
        },
    )
    .await
}

/// Find your carb entries: the largest, the most recent...
#[poise::command(slash_command, user_cooldown = 10)]
#[track_analytics("search_carbs")]
pub async fn carbs(
    ctx: Context<'_>,
    #[description = "Which entries to list first (default: most recent)"] sort: Option<AmountSort>,
    #[description = "How far back to search (default: last 7 days)"] period: Option<SearchPeriod>,
    #[description = "Only entries of at least this many grams"]
    #[min = 0.0]
    min: Option<f64>,
    #[description = "Search another user's data"] user: Option<serenity::User>,
) -> Result<(), Error> {
    run(
        ctx,
        Request {
            kind: Kind::Carbs,
            sort: amount_sort(sort),
            period: period.unwrap_or(SearchPeriod::Week),
            threshold: None,
            min_amount: min.map(|m| m as f32),
            user,
        },
    )
    .await
}

/// Find your insulin doses: the largest, the most recent...
#[poise::command(slash_command, user_cooldown = 10)]
#[track_analytics("search_insulin")]
pub async fn insulin(
    ctx: Context<'_>,
    #[description = "Which doses to list first (default: most recent)"] sort: Option<AmountSort>,
    #[description = "How far back to search (default: last 7 days)"] period: Option<SearchPeriod>,
    #[description = "Only doses of at least this many units"]
    #[min = 0.0]
    min: Option<f64>,
    #[description = "Search another user's data"] user: Option<serenity::User>,
) -> Result<(), Error> {
    run(
        ctx,
        Request {
            kind: Kind::Insulin,
            sort: amount_sort(sort),
            period: period.unwrap_or(SearchPeriod::Week),
            threshold: None,
            min_amount: min.map(|m| m as f32),
            user,
        },
    )
    .await
}

fn amount_sort(sort: Option<AmountSort>) -> Sort {
    match sort.unwrap_or(AmountSort::Recent) {
        AmountSort::Recent => Sort::Recent,
        AmountSort::Largest => Sort::Highest,
        AmountSort::Smallest => Sort::Lowest,
        AmountSort::Oldest => Sort::Oldest,
    }
}

/// One search, as asked for by any of the subcommands.
struct Request {
    kind: Kind,
    sort: Sort,
    period: SearchPeriod,
    /// Glucose threshold in mg/dL replacing the profile's target, for lows
    /// and highs.
    threshold: Option<f32>,
    /// Smallest amount to keep, for carbs and insulin.
    min_amount: Option<f32>,
    user: Option<serenity::User>,
}

/// What a search found, before it is put on screen.
struct Found {
    hits: Vec<Hit>,
    /// The search criteria, worded to follow the result count
    /// (e.g. "below **70 mg/dL** (3.9 mmol/L)").
    criteria: String,
    /// Anything the reader should know about how the results were chosen.
    notes: Vec<String>,
}

/// The live state behind one results message.
struct Session {
    kind: Kind,
    sort: Sort,
    hits: Vec<Hit>,
    page: usize,
    /// The result currently open on a graph, with the window the graph shows.
    viewing: Option<(Hit, DateTime<Utc>, Duration)>,
    period: SearchPeriod,
    criteria: String,
    notes: Vec<String>,
    owner_name: String,
    owner_avatar: Option<String>,
    settings: ProfileSettings,
}

async fn run(ctx: Context<'_>, request: Request) -> Result<(), Error> {
    let target_user = request.user.as_ref().unwrap_or(ctx.author());
    let target_id = target_user.id;

    let user_data = get_db_user!(ctx, target_id.get());

    check_privacy!(ctx, target_id, user_data);

    // Honor the data owner's force_ephemeral preference (see graph.rs).
    let reply_ephemeral = user_data.force_ephemeral;

    let client = get_nightscout_client!(ctx, user_data);

    crate::tips::safe_defer_with(ctx, reply_ephemeral).await?;

    let now = Utc::now();
    let start = now - Duration::days(request.period.days());
    tracing::debug!(
        target_user = %crate::logging::redact(target_id.get()),
        kind = ?request.kind,
        sort = ?request.sort,
        period_days = request.period.days(),
        "searching nightscout history"
    );

    let profile = client.profiles().current().await.ok().flatten();
    let settings = ProfileSettings::from_profile(profile.as_ref());

    let found = if request.kind.is_episode() {
        let entries = match client
            .entries()
            .sgv()
            .list()
            .since(start)
            .limit(ENTRY_LIMIT)
            .send()
            .await
        {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(error = %e, "nightscout SGV request failed");
                send_error!(
                    ctx,
                    "Fetch Error",
                    "Could not retrieve glucose data. Please try again in a moment."
                );
                return Ok(());
            }
        };
        tracing::debug!(count = entries.len(), "fetched SGV entries");

        if entries.is_empty() {
            send_error!(
                ctx,
                "No Data",
                format!(
                    "No glucose entries found in the {}.",
                    request.period.label().to_lowercase()
                )
            );
            return Ok(());
        }

        let truncated = entries.len() >= ENTRY_LIMIT;
        let mut readings: Vec<Reading> = entries
            .iter()
            .map(|e| Reading {
                date: e.date.to_datetime(),
                sgv: e.sgv.as_mgdl() as f32,
            })
            .collect();
        readings.sort_by_key(|r| r.date);

        let mut found = find_glucose(&request, &readings, settings, now);
        if truncated {
            found.notes.push(oldest_note(
                readings.first().map(|r| r.date),
                settings.timezone,
            ));
        }
        found
    } else {
        let treatments = match client
            .treatments()
            .list()
            .since(start)
            .limit(TREATMENT_LIMIT)
            .send()
            .await
        {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(error = %e, "nightscout treatments request failed");
                send_error!(
                    ctx,
                    "Fetch Error",
                    "Could not retrieve your treatments. Please try again in a moment."
                );
                return Ok(());
            }
        };
        tracing::debug!(count = treatments.len(), "fetched treatments");

        let truncated = treatments.len() >= TREATMENT_LIMIT;
        let treatments: Vec<GraphTreatment> = treatments
            .into_iter()
            .filter_map(|t| GraphTreatment::try_from(t).ok())
            .collect();

        let mut found = find_treatments(&request, &treatments, &user_data);
        if truncated {
            found.notes.push(oldest_note(
                treatments.iter().map(|t| t.date).min(),
                settings.timezone,
            ));
        }
        found
    };

    let mut session = Session {
        kind: request.kind,
        sort: request.sort,
        hits: found.hits,
        page: 0,
        viewing: None,
        period: request.period,
        criteria: found.criteria,
        notes: found.notes,
        owner_name: target_user.name.clone(),
        owner_avatar: target_user.avatar_url(),
        settings,
    };
    sort_hits(&mut session.hits, session.sort);
    tracing::debug!(hits = session.hits.len(), "search finished");

    if session.hits.is_empty() {
        ctx.send(
            poise::CreateReply::default()
                .embed(empty_embed(&session))
                .ephemeral(reply_ephemeral),
        )
        .await?;
        return Ok(());
    }

    let reply_handle = ctx
        .send(
            poise::CreateReply::default()
                .embed(build_embed(&session, false))
                .components(build_components(&session))
                .ephemeral(reply_ephemeral),
        )
        .await?;
    let message = reply_handle.message().await?;

    let http = &ctx.serenity_context().http;
    let mut collector =
        serenity::ComponentInteractionCollector::new(ctx.serenity_context().shard.clone())
            .message_id(message.id)
            .stream();

    // The latest click's token outlives the command's own, so closing the
    // search goes through it when there is one.
    let mut last_interaction: Option<ComponentInteraction> = None;

    while let Ok(Some(interaction)) = tokio::time::timeout(IDLE_TIMEOUT, collector.next()).await {
        if interaction.user.id != ctx.author().id {
            interaction
                .create_response(
                    http,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .content(
                                "Only the person who ran this search can use it. \
                                 Run `/search` to start your own.",
                            )
                            .ephemeral(true),
                    ),
                )
                .await?;
            continue;
        }

        interaction.defer(http).await?;

        // Which result to open on a graph, if the click asks for one.
        let mut open: Option<usize> = None;
        match interaction.data.custom_id.as_str() {
            ID_VIEW => {
                if let ComponentInteractionDataKind::StringSelect { values } =
                    &interaction.data.kind
                {
                    open = values.first().and_then(|v| v.parse::<usize>().ok());
                }
            }
            ID_PREV_HIT => open = session.viewing_index().and_then(|i| i.checked_sub(1)),
            ID_NEXT_HIT => open = session.viewing_index().map(|i| i + 1),
            ID_SORT => {
                if let ComponentInteractionDataKind::StringSelect { values } =
                    &interaction.data.kind
                    && let Some(sort) = values
                        .first()
                        .and_then(|v| v.parse::<usize>().ok())
                        .and_then(|i| session.kind.sorts().get(i))
                {
                    session.sort = *sort;
                    sort_hits(&mut session.hits, session.sort);
                    // Stay with the open result, wherever it landed.
                    session.page = session.viewing_index().unwrap_or(0) / PAGE_SIZE;
                }
            }
            ID_PREV_PAGE => session.page = session.page.saturating_sub(1),
            ID_NEXT_PAGE => session.page = (session.page + 1).min(session.page_count() - 1),
            ID_CLOSE => {
                session.viewing = None;
                interaction
                    .edit_response(
                        http,
                        EditInteractionResponse::new()
                            .embed(build_embed(&session, false))
                            .components(build_components(&session))
                            .clear_attachments(),
                    )
                    .await?;
                last_interaction = Some(interaction);
                continue;
            }
            _ => {}
        }

        let graph = match open.filter(|i| *i < session.hits.len()) {
            Some(index) => match render_hit(ctx, &client, &user_data, &session, index).await {
                Ok((png, start, length)) => {
                    session.viewing = Some((session.hits[index].clone(), start, length));
                    session.page = index / PAGE_SIZE;
                    Some(png)
                }
                Err(message) => {
                    interaction
                        .create_followup(
                            http,
                            CreateInteractionResponseFollowup::new()
                                .embed(
                                    CreateEmbed::new()
                                        .title(format!(
                                            "{} Could Not Open That Result",
                                            emojis::error()
                                        ))
                                        .description(message)
                                        .color(Colour::RED),
                                )
                                .ephemeral(true),
                        )
                        .await?;
                    None
                }
            },
            None => None,
        };

        // Without a new attachment the edit leaves the current graph in place.
        let mut edit = EditInteractionResponse::new()
            .embed(build_embed(&session, false))
            .components(build_components(&session));
        if let Some(png) = graph {
            edit = edit.new_attachment(CreateAttachment::bytes(png, GRAPH_FILENAME));
        }
        interaction.edit_response(http, edit).await?;
        last_interaction = Some(interaction);
    }

    // Idle for too long: leave the results (and any open graph) on screen but
    // take the controls away. The message may be gone by now, so a failure
    // here is not worth reporting.
    let closed = build_embed(&session, true);
    let result = match last_interaction {
        Some(interaction) => interaction
            .edit_response(
                http,
                EditInteractionResponse::new()
                    .embed(closed)
                    .components(vec![]),
            )
            .await
            .map(|_| ()),
        None => {
            reply_handle
                .edit(
                    ctx,
                    poise::CreateReply::default()
                        .embed(closed)
                        .components(vec![]),
                )
                .await
        }
    };
    if let Err(e) = result {
        tracing::debug!(error = %e, "could not close search results");
    }

    Ok(())
}

/// Finds the glucose episodes a lows, highs or in-range search asks for.
fn find_glucose(
    request: &Request,
    readings: &[Reading],
    settings: ProfileSettings,
    now: DateTime<Utc>,
) -> Found {
    let is_mmol = settings.is_mmol;
    let (low, high) = (settings.target_low, settings.target_high);

    let (hits, criteria) = match request.kind {
        Kind::Lows => {
            let threshold = request.threshold.unwrap_or(low);
            (
                find_episodes(readings, |sgv| sgv < threshold, Extreme::Min, now),
                format!("below {}", glucose(threshold, is_mmol, true)),
            )
        }
        Kind::Highs => {
            let threshold = request.threshold.unwrap_or(high);
            (
                find_episodes(readings, |sgv| sgv > threshold, Extreme::Max, now),
                format!("above {}", glucose(threshold, is_mmol, true)),
            )
        }
        _ => (
            find_episodes(
                readings,
                |sgv| (low..=high).contains(&sgv),
                Extreme::Mean,
                now,
            ),
            format!("between {}", glucose_range(low, high, is_mmol)),
        ),
    };

    Found {
        hits,
        criteria,
        notes: vec![format!(
            "Anything shorter than {MIN_EPISODE_MINUTES} minutes is not counted."
        )],
    }
}

/// Finds the treatments a carbs or insulin search asks for.
fn find_treatments(
    request: &Request,
    treatments: &[GraphTreatment],
    user_data: &UserDecrypted,
) -> Found {
    let kind = request.kind;
    let mut notes = Vec::new();

    let (hits, criteria) = match request.min_amount {
        Some(min) => (
            treatment_hits(treatments, kind, |amount| amount >= min),
            format!("of **{}** or more", amount(min, kind)),
        ),
        // The data owner chose not to see microboluses: keep them out of
        // their insulin results too, unless a minimum was asked for.
        None if kind == Kind::Insulin && !user_data.display_microbolus => {
            let threshold = user_data.microbolus_threshold as f32;
            notes.push(format!(
                "Microboluses of {} or less are hidden. Set `min` to see them.",
                amount(threshold, kind)
            ));
            (
                treatment_hits(treatments, kind, |amount| amount > threshold),
                String::new(),
            )
        }
        None => (treatment_hits(treatments, kind, |_| true), String::new()),
    };

    Found {
        hits,
        criteria,
        notes,
    }
}

/// Fetches and renders the graph for one result. The error is a message fit
/// to show the user.
async fn render_hit(
    ctx: Context<'_>,
    client: &cinnamon::Client,
    user_data: &UserDecrypted,
    session: &Session,
    index: usize,
) -> Result<(Vec<u8>, DateTime<Utc>, Duration), String> {
    let (start, length) = search::graph_window(&session.hits[index], session.kind, Utc::now());

    // A little before the window, so the trace does not start mid-air.
    let data = graph_render::fetch_window(client, start - Duration::minutes(15), start + length)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "failed to fetch glucose entries for search result");
            "Could not retrieve glucose data. Please try again in a moment.".to_string()
        })?;
    if data.entries.is_empty() {
        return Err("There is no glucose data around that moment to draw.".to_string());
    }

    let png = graph_render::render_png(
        &ctx.data().database,
        user_data,
        session.settings,
        data,
        GraphWindow {
            start,
            duration: length,
            pinned: true,
        },
        Strength::from_level(user_data.graph_denoise),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "failed to render search result graph");
        "Something went wrong drawing that graph. Please try again.".to_string()
    })?;

    Ok((png, start, length))
}

impl Session {
    fn page_count(&self) -> usize {
        self.hits.len().div_ceil(PAGE_SIZE).max(1)
    }

    /// Where the open result sits in the current order.
    fn viewing_index(&self) -> Option<usize> {
        let (viewing, ..) = self.viewing.as_ref()?;
        self.hits.iter().position(|h| h == viewing)
    }

    fn page_hits(&self) -> impl Iterator<Item = (usize, &Hit)> {
        self.hits
            .iter()
            .enumerate()
            .skip(self.page * PAGE_SIZE)
            .take(PAGE_SIZE)
    }

    fn title(&self) -> String {
        format!("🔎 {} · {}", kind_name(self.kind), self.period.label())
    }

    fn author(&self) -> CreateEmbedAuthor {
        let author = CreateEmbedAuthor::new(&self.owner_name);
        match &self.owner_avatar {
            Some(url) => author.icon_url(url),
            None => author,
        }
    }
}

fn empty_embed(session: &Session) -> CreateEmbed {
    let (_, plural) = kind_nouns(session.kind);
    let period = session.period.label().to_lowercase();

    let mut description = if session.criteria.is_empty() {
        format!("No {plural} found in the {period}.")
    } else {
        format!("No {plural} {} in the {period}.", session.criteria)
    };
    for note in &session.notes {
        description.push_str(&format!("\n-# {note}"));
    }
    if !matches!(session.period, SearchPeriod::Quarter) {
        description.push_str("\n\nTry a longer `period` to look further back.");
    }

    CreateEmbed::new()
        .author(session.author())
        .title(session.title())
        .description(description)
        .color(Colour::LIGHT_GREY)
}

fn build_embed(session: &Session, closed: bool) -> CreateEmbed {
    let is_mmol = session.settings.is_mmol;
    let tz = session.settings.timezone;
    let count = session.hits.len();
    let (singular, plural) = kind_nouns(session.kind);

    let mut summary = format!("**{count}** {}", if count == 1 { singular } else { plural });
    if !session.criteria.is_empty() {
        summary.push_str(&format!(" {}", session.criteria));
    }
    summary.push_str(&format!(" · {}", total(session)));

    let viewing_index = session.viewing_index();
    let rows: Vec<String> = session
        .page_hits()
        .map(|(i, hit)| {
            let row = format!(
                "`{}.` **{}** · {}",
                i + 1,
                when(hit.start, tz),
                detail(hit, session.kind, is_mmol, true)
            );
            // The open result stands out as a quote.
            if viewing_index == Some(i) {
                format!("> {row}")
            } else {
                row
            }
        })
        .collect();

    let mut description = format!(
        "{summary}\nSorted by **{}**\n\n{}",
        sort_label(session.sort, session.kind).to_lowercase(),
        rows.join("\n")
    );
    if !session.notes.is_empty() {
        description.push('\n');
        for note in &session.notes {
            description.push_str(&format!("\n-# {note}"));
        }
    }

    let mut embed = CreateEmbed::new()
        .author(session.author())
        .title(session.title())
        .description(description)
        .color(kind_color(session.kind));

    if let Some((hit, start, length)) = &session.viewing {
        let name = match viewing_index {
            Some(i) => format!("Result {} of {count}", i + 1),
            None => "On the graph".to_string(),
        };
        let span = if session.kind.is_episode() {
            format!(
                "{} → {}",
                when(hit.start, tz),
                hit.end.with_timezone(&tz).format("%H:%M")
            )
        } else {
            when(hit.start, tz)
        };
        embed = embed
            .field(
                name,
                format!(
                    "**{span}** · {}\n-# Showing {} from {}.",
                    detail(hit, session.kind, is_mmol, true),
                    duration(*length),
                    when(*start, tz)
                ),
                false,
            )
            .image(format!("attachment://{GRAPH_FILENAME}"));
    }

    let footer = if closed {
        "Search closed · Run /search to look again".to_string()
    } else {
        format!("Times in {}", tz.name())
    };

    embed.footer(CreateEmbedFooter::new(footer))
}

fn build_components(session: &Session) -> Vec<CreateActionRow> {
    let is_mmol = session.settings.is_mmol;
    let tz = session.settings.timezone;
    let viewing_index = session.viewing_index();
    let mut rows = Vec::new();

    let options: Vec<CreateSelectMenuOption> = session
        .page_hits()
        .map(|(i, hit)| {
            CreateSelectMenuOption::new(
                format!("{}. {}", i + 1, when(hit.start, tz)),
                i.to_string(),
            )
            .description(detail(hit, session.kind, is_mmol, false))
            .default_selection(viewing_index == Some(i))
        })
        .collect();
    rows.push(CreateActionRow::SelectMenu(
        CreateSelectMenu::new(ID_VIEW, CreateSelectMenuKind::String { options })
            .placeholder("Open a result on a graph…"),
    ));

    if session.hits.len() > 1 {
        let options: Vec<CreateSelectMenuOption> = session
            .kind
            .sorts()
            .iter()
            .enumerate()
            .map(|(i, sort)| {
                CreateSelectMenuOption::new(
                    format!("Sort: {}", sort_label(*sort, session.kind)),
                    i.to_string(),
                )
                .default_selection(*sort == session.sort)
            })
            .collect();
        rows.push(CreateActionRow::SelectMenu(
            CreateSelectMenu::new(ID_SORT, CreateSelectMenuKind::String { options })
                .placeholder("Sort results…"),
        ));
    }

    let pages = session.page_count();
    if pages > 1 {
        rows.push(CreateActionRow::Buttons(vec![
            CreateButton::new(ID_PREV_PAGE)
                .label("◀")
                .style(ButtonStyle::Secondary)
                .disabled(session.page == 0),
            CreateButton::new(ID_PAGE_LABEL)
                .label(format!("Page {} of {pages}", session.page + 1))
                .style(ButtonStyle::Secondary)
                .disabled(true),
            CreateButton::new(ID_NEXT_PAGE)
                .label("▶")
                .style(ButtonStyle::Secondary)
                .disabled(session.page + 1 >= pages),
        ]));
    }

    if session.viewing.is_some() {
        rows.push(CreateActionRow::Buttons(vec![
            CreateButton::new(ID_PREV_HIT)
                .label("Previous result")
                .style(ButtonStyle::Primary)
                .disabled(viewing_index.is_none_or(|i| i == 0)),
            CreateButton::new(ID_NEXT_HIT)
                .label("Next result")
                .style(ButtonStyle::Primary)
                .disabled(viewing_index.is_none_or(|i| i + 1 >= session.hits.len())),
            CreateButton::new(ID_CLOSE)
                .label("Close graph")
                .style(ButtonStyle::Secondary),
        ]));
    }

    rows
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Lows => "Lows",
        Kind::Highs => "Highs",
        Kind::InRange => "In range",
        Kind::Carbs => "Carbs",
        Kind::Insulin => "Insulin",
    }
}

/// What one result is called, singular and plural.
fn kind_nouns(kind: Kind) -> (&'static str, &'static str) {
    match kind {
        Kind::Lows => ("low", "lows"),
        Kind::Highs => ("high", "highs"),
        Kind::InRange => ("stretch in range", "stretches in range"),
        Kind::Carbs => ("carb entry", "carb entries"),
        Kind::Insulin => ("insulin dose", "insulin doses"),
    }
}

/// Low, high and in-range share their colors with `/bg`.
fn kind_color(kind: Kind) -> Colour {
    match kind {
        Kind::Lows => Colour::from_rgb(235, 47, 47),
        Kind::Highs => Colour::from_rgb(227, 177, 11),
        Kind::InRange => Colour::from_rgb(87, 189, 79),
        Kind::Carbs | Kind::Insulin => Colour::BLURPLE,
    }
}

fn sort_label(sort: Sort, kind: Kind) -> &'static str {
    match (sort, kind.is_episode()) {
        (Sort::Recent, _) => "Most recent first",
        (Sort::Oldest, _) => "Oldest first",
        (Sort::Longest, _) => "Longest first",
        (Sort::Shortest, _) => "Shortest first",
        (Sort::Highest, true) => "Highest first",
        (Sort::Lowest, true) => "Lowest first",
        (Sort::Highest, false) => "Largest first",
        (Sort::Lowest, false) => "Smallest first",
    }
}

/// What all the results add up to.
fn total(session: &Session) -> String {
    if session.kind.is_episode() {
        let sum = session
            .hits
            .iter()
            .fold(Duration::zero(), |sum, h| sum + h.duration());
        format!("**{}** in total", duration(sum))
    } else {
        let sum: f32 = session.hits.iter().map(|h| h.value).sum();
        format!("**{}** in total", amount(sum, session.kind))
    }
}

/// What sets one result apart, e.g. "45 min · down to **52 mg/dL** (2.9 mmol/L)".
fn detail(hit: &Hit, kind: Kind, is_mmol: bool, markdown: bool) -> String {
    let bold = |text: String| {
        if markdown {
            format!("**{text}**")
        } else {
            text
        }
    };

    let (verb, companion_kind) = match kind {
        Kind::Lows => ("down to", None),
        Kind::Highs => ("up to", None),
        Kind::InRange => ("averaging", None),
        Kind::Carbs => ("", Some(Kind::Insulin)),
        Kind::Insulin => ("", Some(Kind::Carbs)),
    };

    match companion_kind {
        None => {
            let mut text = format!(
                "{} · {verb} {}",
                duration(hit.duration()),
                glucose(hit.value, is_mmol, markdown)
            );
            if hit.ongoing {
                text.push_str(" · ongoing");
            }
            text
        }
        Some(companion_kind) => {
            let mut text = bold(amount(hit.value, kind));
            if let Some(companion) = hit.companion {
                text.push_str(&format!(" with {}", amount(companion, companion_kind)));
            }
            text
        }
    }
}

/// A glucose value in the profile's unit, then the other one in brackets.
fn glucose(mgdl: f32, is_mmol: bool, markdown: bool) -> String {
    let mg = format!("{mgdl:.0} mg/dL");
    let mmol = format!("{:.1} mmol/L", mgdl / 18.0);
    let (primary, secondary) = if is_mmol { (mmol, mg) } else { (mg, mmol) };
    if markdown {
        format!("**{primary}** ({secondary})")
    } else {
        format!("{primary} ({secondary})")
    }
}

/// A glucose range in the profile's unit, then the other one in brackets.
fn glucose_range(low: f32, high: f32, is_mmol: bool) -> String {
    let mg = format!("{low:.0}–{high:.0} mg/dL");
    let mmol = format!("{:.1}–{:.1} mmol/L", low / 18.0, high / 18.0);
    let (primary, secondary) = if is_mmol { (mmol, mg) } else { (mg, mmol) };
    format!("**{primary}** ({secondary})")
}

/// Grams of carbs or units of insulin.
fn amount(value: f32, kind: Kind) -> String {
    if kind == Kind::Insulin {
        let units = format!("{value:.2}");
        let units = units.trim_end_matches('0').trim_end_matches('.');
        format!("{units} U")
    } else {
        format!("{value:.0} g")
    }
}

fn duration(d: Duration) -> String {
    let minutes = d.num_minutes().max(0);
    let (days, hours, mins) = (minutes / 1440, (minutes % 1440) / 60, minutes % 60);
    match (days, hours, mins) {
        (0, 0, m) => format!("{m} min"),
        (0, h, 0) => format!("{h} h"),
        (0, h, m) => format!("{h} h {m} min"),
        (d, 0, _) => format!("{d} d"),
        (d, h, _) => format!("{d} d {h} h"),
    }
}

/// A moment in the data owner's timezone, e.g. "Mon 28 Sep, 03:15".
fn when(date: DateTime<Utc>, tz: Tz) -> String {
    date.with_timezone(&tz)
        .format("%a %-d %b, %H:%M")
        .to_string()
}

/// Tells the reader the search could not reach the start of the period.
fn oldest_note(oldest: Option<DateTime<Utc>>, tz: Tz) -> String {
    match oldest {
        Some(date) => format!(
            "There is too much data to search the whole period: results only go back to {}.",
            when(date, tz)
        ),
        None => "There is too much data to search the whole period.".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_durations() {
        assert_eq!(duration(Duration::minutes(45)), "45 min");
        assert_eq!(duration(Duration::minutes(120)), "2 h");
        assert_eq!(duration(Duration::minutes(130)), "2 h 10 min");
        assert_eq!(duration(Duration::hours(24)), "1 d");
        assert_eq!(duration(Duration::minutes(28 * 60 + 20)), "1 d 4 h");
    }

    #[test]
    fn formats_glucose_in_the_profile_unit_first() {
        assert_eq!(glucose(54.0, false, true), "**54 mg/dL** (3.0 mmol/L)");
        assert_eq!(glucose(54.0, true, false), "3.0 mmol/L (54 mg/dL)");
        assert_eq!(
            glucose_range(70.0, 180.0, true),
            "**3.9–10.0 mmol/L** (70–180 mg/dL)"
        );
    }

    #[test]
    fn formats_amounts() {
        assert_eq!(amount(45.0, Kind::Carbs), "45 g");
        assert_eq!(amount(4.5, Kind::Insulin), "4.5 U");
        assert_eq!(amount(4.0, Kind::Insulin), "4 U");
        assert_eq!(amount(0.05, Kind::Insulin), "0.05 U");
    }
}
