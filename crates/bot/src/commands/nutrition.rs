use crate::data::{Context, Error};
use crate::utils::emojis;
use futures::StreamExt;
use poise::serenity_prelude as serenity;
use serde::Deserialize;
use serenity::all::{
    ButtonStyle, Colour, ComponentInteraction, ComponentInteractionDataKind, CreateActionRow,
    CreateButton, CreateEmbed, CreateEmbedFooter, CreateInteractionResponse,
    CreateInteractionResponseMessage, CreateSelectMenu, CreateSelectMenuKind,
    CreateSelectMenuOption, EditInteractionResponse, Message, MessageId,
    MessageInteractionMetadata, UserId,
};
use std::env;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, warn};

const TOKEN_URL: &str = "https://oauth.fatsecret.com/connect/token";
const API_URL: &str = "https://platform.fatsecret.com/rest/server.api";
const SEARCH_LIMIT: usize = 25;
const COLLECTOR_TIMEOUT: Duration = Duration::from_secs(180);

struct CachedToken {
    token: String,
    expires_at: Instant,
}

static TOKEN_CACHE: OnceLock<Mutex<Option<CachedToken>>> = OnceLock::new();

fn token_cache() -> &'static Mutex<Option<CachedToken>> {
    TOKEN_CACHE.get_or_init(|| Mutex::new(None))
}

#[derive(Deserialize)]
struct TokenResp {
    access_token: String,
    expires_in: u64,
}

async fn get_access_token(http: &reqwest::Client) -> Result<String, Error> {
    let mut guard = token_cache().lock().await;

    if let Some(cached) = guard.as_ref()
        && cached.expires_at > Instant::now() + Duration::from_secs(30)
    {
        return Ok(cached.token.clone());
    }

    let client_id = env::var("FOOD_CLIENT_ID")?;
    let client_secret = env::var("FOOD_CLIENT_SECRET")?;

    let resp = http
        .post(TOKEN_URL)
        .basic_auth(&client_id, Some(&client_secret))
        .form(&[("grant_type", "client_credentials"), ("scope", "basic")])
        .send()
        .await?
        .error_for_status()?;

    let body: TokenResp = resp.json().await?;
    let token = body.access_token.clone();
    *guard = Some(CachedToken {
        token: body.access_token,
        expires_at: Instant::now() + Duration::from_secs(body.expires_in),
    });

    Ok(token)
}

#[derive(Deserialize)]
struct FsError {
    message: String,
}

#[derive(Deserialize)]
struct FoodSearchResponse {
    foods: Option<FoodsBlock>,
    error: Option<FsError>,
}

#[derive(Deserialize)]
struct FoodsBlock {
    #[serde(default, deserialize_with = "de_one_or_many")]
    food: Vec<SearchFood>,
}

#[derive(Deserialize, Clone)]
struct SearchFood {
    food_id: String,
    food_name: String,
    #[serde(default)]
    food_type: Option<String>,
    #[serde(default)]
    brand_name: Option<String>,
    food_description: String,
    #[serde(default)]
    food_url: Option<String>,
}

#[derive(Deserialize)]
struct GetFoodResponse {
    food: Option<DetailFood>,
    error: Option<FsError>,
}

#[derive(Deserialize, Clone)]
struct DetailFood {
    food_name: String,
    #[serde(default)]
    brand_name: Option<String>,
    #[serde(default)]
    food_url: Option<String>,
    servings: ServingsBlock,
}

#[derive(Deserialize, Clone)]
struct ServingsBlock {
    #[serde(default, deserialize_with = "de_one_or_many")]
    serving: Vec<Serving>,
}

#[derive(Deserialize, Clone)]
struct Serving {
    serving_description: String,
    #[serde(default)]
    metric_serving_amount: Option<String>,
    #[serde(default)]
    metric_serving_unit: Option<String>,
    #[serde(default)]
    calories: Option<String>,
    #[serde(default)]
    carbohydrate: Option<String>,
    #[serde(default)]
    fat: Option<String>,
    #[serde(default)]
    saturated_fat: Option<String>,
    #[serde(default)]
    protein: Option<String>,
    #[serde(default)]
    fiber: Option<String>,
    #[serde(default)]
    sugar: Option<String>,
    #[serde(default)]
    sodium: Option<String>,
    #[serde(default)]
    cholesterol: Option<String>,
}

fn de_one_or_many<'de, T, D>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    T: serde::de::DeserializeOwned,
    D: serde::Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(deserializer)?;
    match v {
        serde_json::Value::Null => Ok(Vec::new()),
        serde_json::Value::Array(_) => serde_json::from_value(v).map_err(serde::de::Error::custom),
        _ => {
            let single: T = serde_json::from_value(v).map_err(serde::de::Error::custom)?;
            Ok(vec![single])
        }
    }
}

fn parse_num(s: Option<&str>) -> Option<f64> {
    s.and_then(|x| x.parse::<f64>().ok())
}

fn fmt_g(s: Option<&str>) -> String {
    match parse_num(s) {
        Some(n) if n.fract().abs() < 0.05 => format!("{:.0} g", n),
        Some(n) => format!("{:.1} g", n),
        None => "-".into(),
    }
}

fn fmt_mg(s: Option<&str>) -> String {
    match parse_num(s) {
        Some(n) => format!("{:.0} mg", n),
        None => "-".into(),
    }
}

fn fmt_kcal(s: Option<&str>) -> String {
    match parse_num(s) {
        Some(n) => format!("{:.0} kcal", n),
        None => "-".into(),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

fn food_title(name: &str, brand: Option<&str>) -> String {
    match brand.filter(|b| !b.is_empty()) {
        Some(b) => format!("{} {} - {}", emojis::nutrition(), name, b),
        None => format!("{} {}", emojis::nutrition(), name),
    }
}

/// Prefer the per-100 g/ml metric serving for comparability across foods.
fn pick_serving(servings: &[Serving]) -> Option<&Serving> {
    servings
        .iter()
        .find(|s| {
            parse_num(s.metric_serving_amount.as_deref())
                .map(|n| (n - 100.0).abs() < 0.01)
                .unwrap_or(false)
                && matches!(s.metric_serving_unit.as_deref(), Some("g" | "ml"))
        })
        .or_else(|| servings.first())
}

fn score_match(query: &str, food: &SearchFood) -> i64 {
    let q = query.trim().to_lowercase();
    let n = food.food_name.to_lowercase();
    let mut s = 0i64;
    if n == q {
        s += 10_000;
    } else if n.starts_with(&q) {
        s += 1_000;
    } else if n.split_whitespace().any(|w| w == q) {
        s += 500;
    } else if n.contains(&q) {
        s += 100;
    }
    if food.food_type.as_deref() == Some("Generic") {
        s += 50;
    }
    s
}

fn pick_best_index(query: &str, foods: &[SearchFood]) -> usize {
    foods
        .iter()
        .enumerate()
        .max_by_key(|(i, f)| (score_match(query, f), -(*i as i64)))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

async fn search_foods(
    http: &reqwest::Client,
    token: &str,
    query: &str,
) -> Result<Vec<SearchFood>, Error> {
    let limit_str = SEARCH_LIMIT.to_string();
    let body: FoodSearchResponse = http
        .get(API_URL)
        .bearer_auth(token)
        .query(&[
            ("method", "foods.search"),
            ("search_expression", query),
            ("format", "json"),
            ("max_results", limit_str.as_str()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    if let Some(e) = body.error {
        return Err(anyhow::anyhow!("FatSecret search error: {}", e.message));
    }
    Ok(body.foods.map(|b| b.food).unwrap_or_default())
}

async fn fetch_detail(
    http: &reqwest::Client,
    token: &str,
    food_id: &str,
) -> Result<DetailFood, Error> {
    let body: GetFoodResponse = http
        .get(API_URL)
        .bearer_auth(token)
        .query(&[
            ("method", "food.get.v2"),
            ("food_id", food_id),
            ("format", "json"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    if let Some(e) = body.error {
        return Err(anyhow::anyhow!("FatSecret food.get error: {}", e.message));
    }
    body.food
        .ok_or_else(|| anyhow::anyhow!("No food data in FatSecret response"))
}

#[derive(Clone, Copy, PartialEq)]
enum DisplayMode {
    Fast,
    Full,
}

fn build_fast_embed(food: &DetailFood) -> CreateEmbed {
    let mut e = CreateEmbed::new()
        .title(food_title(&food.food_name, food.brand_name.as_deref()))
        .color(Colour::from_rgb(87, 189, 79))
        .footer(
            CreateEmbedFooter::new("Powered by FatSecret Platform API - platform.fatsecret.com")
                .icon_url("https://platform.fatsecret.com/api/static/images/powered_by_fatsecret_square_brand.png"),
        );

    if let Some(url) = food.food_url.as_deref().filter(|u| !u.is_empty()) {
        e = e.url(url);
    }

    let Some(s) = pick_serving(&food.servings.serving) else {
        return e.description("_No serving data available._");
    };

    e.description(format!("**Per {}**", s.serving_description))
        .field(
            format!("{} Calories", emojis::nutrition()),
            fmt_kcal(s.calories.as_deref()),
            true,
        )
        .field(
            format!("{} Carbs", emojis::carbs()),
            fmt_g(s.carbohydrate.as_deref()),
            true,
        )
}

fn build_full_embed(food: &DetailFood) -> CreateEmbed {
    let mut e = CreateEmbed::new()
        .title(food_title(&food.food_name, food.brand_name.as_deref()))
        .color(Colour::from_rgb(87, 189, 79))
        .footer(
            CreateEmbedFooter::new("Powered by FatSecret Platform API - platform.fatsecret.com")
                .icon_url("https://platform.fatsecret.com/api/static/images/powered_by_fatsecret_square_brand.png"),
        );

    if let Some(url) = food.food_url.as_deref().filter(|u| !u.is_empty()) {
        e = e.url(url);
    }

    let Some(s) = pick_serving(&food.servings.serving) else {
        return e.description("_No serving data available._");
    };

    e = e
        .description(format!("**Per {}**", s.serving_description))
        .field(
            format!("{} Calories", emojis::nutrition()),
            fmt_kcal(s.calories.as_deref()),
            true,
        )
        .field(
            format!("{} Carbs", emojis::carbs()),
            fmt_g(s.carbohydrate.as_deref()),
            true,
        )
        .field(
            format!("{} Protein", emojis::protein()),
            fmt_g(s.protein.as_deref()),
            true,
        )
        .field(
            format!("{} Fat", emojis::fat()),
            fmt_g(s.fat.as_deref()),
            true,
        )
        .field(
            format!("{} Sugar", emojis::sugar()),
            fmt_g(s.sugar.as_deref()),
            true,
        )
        .field(
            format!("{} Fiber", emojis::fibers()),
            fmt_g(s.fiber.as_deref()),
            true,
        );

    if parse_num(s.saturated_fat.as_deref())
        .filter(|n| *n > 0.0)
        .is_some()
    {
        e = e.field(
            format!("{} Sat. Fat", emojis::fat()),
            fmt_g(s.saturated_fat.as_deref()),
            true,
        );
    }
    if parse_num(s.sodium.as_deref())
        .filter(|n| *n > 0.0)
        .is_some()
    {
        e = e.field(
            format!("{} Sodium", emojis::salt()),
            fmt_mg(s.sodium.as_deref()),
            true,
        );
    }
    if parse_num(s.cholesterol.as_deref())
        .filter(|n| *n > 0.0)
        .is_some()
    {
        e = e.field(
            format!("{} Cholesterol", emojis::fat()),
            fmt_mg(s.cholesterol.as_deref()),
            true,
        );
    }

    e
}

fn build_mode_embed(food: &DetailFood, mode: DisplayMode) -> CreateEmbed {
    match mode {
        DisplayMode::Fast => build_fast_embed(food),
        DisplayMode::Full => build_full_embed(food),
    }
}

fn build_search_fallback_embed(f: &SearchFood) -> CreateEmbed {
    let mut e = CreateEmbed::new()
        .title(food_title(&f.food_name, f.brand_name.as_deref()))
        .description(&f.food_description)
        .color(Colour::from_rgb(87, 189, 79))
        .footer(
            CreateEmbedFooter::new("Powered by FatSecret Platform API - platform.fatsecret.com")
                .icon_url("https://platform.fatsecret.com/api/static/images/powered_by_fatsecret_square_brand.png"),
        );
    if let Some(url) = f.food_url.as_deref().filter(|u| !u.is_empty()) {
        e = e.url(url);
    }
    e
}

fn build_mode_buttons(base: &str, food_id: &str, mode: DisplayMode) -> CreateActionRow {
    CreateActionRow::Buttons(vec![
        CreateButton::new(format!("{base}_{food_id}_fast"))
            .label("Fast Info")
            .style(ButtonStyle::Primary)
            .disabled(mode == DisplayMode::Fast),
        CreateButton::new(format!("{base}_{food_id}_full"))
            .label("Full Info")
            .style(ButtonStyle::Primary)
            .disabled(mode == DisplayMode::Full),
    ])
}

fn build_alternatives_menu(
    custom_id: &str,
    foods: &[SearchFood],
    current_id: &str,
) -> Option<CreateActionRow> {
    if foods.len() <= 1 {
        return None;
    }
    let options: Vec<CreateSelectMenuOption> = foods
        .iter()
        .take(25)
        .map(|f| {
            let label = match f.brand_name.as_deref().filter(|b| !b.is_empty()) {
                Some(b) => truncate(&format!("{} - {}", f.food_name, b), 100),
                None => truncate(&f.food_name, 100),
            };
            let mut opt = CreateSelectMenuOption::new(label, &f.food_id)
                .description(truncate(&f.food_description, 100));
            if f.food_id == current_id {
                opt = opt.default_selection(true);
            }
            opt
        })
        .collect();

    Some(CreateActionRow::SelectMenu(
        CreateSelectMenu::new(custom_id, CreateSelectMenuKind::String { options })
            .placeholder("Other matches…"),
    ))
}

fn build_components(
    base: &str,
    mode: DisplayMode,
    has_detail: bool,
    foods: &[SearchFood],
    current_id: &str,
) -> Vec<CreateActionRow> {
    let mut rows = Vec::new();
    if has_detail {
        rows.push(build_mode_buttons(base, current_id, mode));
    }
    if let Some(menu) = build_alternatives_menu(&format!("{base}_select"), foods, current_id) {
        rows.push(menu);
    }
    rows
}

/// Look up basic nutrition info for a food (powered by FatSecret).
#[poise::command(
    slash_command,
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
pub async fn nutrition(
    ctx: Context<'_>,
    #[description = "Exact food name to look up"] food: String,
) -> Result<(), Error> {
    let __start = std::time::Instant::now();
    let query = food.trim();
    if query.is_empty() {
        send_error!(ctx, "Empty Query", "Please provide a food name to look up.");
        return Ok(());
    }
    debug!(query, "nutrition lookup requested");

    // Honor force_ephemeral if the user has an account; looking up food does not
    // require one, so default to a public reply when there is no row.
    let reply_ephemeral = ctx
        .data()
        .database
        .get_user(ctx.author().id.get())
        .await?
        .is_some_and(|u| u.force_ephemeral);

    crate::tips::safe_defer_with(ctx, reply_ephemeral).await?;

    let http = crate::utils::net::shared_client().clone();

    let token = match get_access_token(&http).await {
        Ok(t) => t,
        Err(e) => {
            warn!(error = %e, "failed to get FatSecret token");
            send_error!(
                ctx,
                "Service Unavailable",
                "Could not authenticate with the nutrition service."
            );
            return Ok(());
        }
    };

    let foods = match search_foods(&http, &token, query).await {
        Ok(f) => f,
        Err(e) => {
            warn!(error = %e, "nutrition search failed");
            send_error!(ctx, "Search Failed", "Failed to reach nutrition service.");
            return Ok(());
        }
    };

    debug!(query, results = foods.len(), "nutrition search complete");

    if foods.is_empty() {
        send_error!(
            ctx,
            "Not Found",
            format!("No results found for **{}**.", query)
        );
        return Ok(());
    }

    let best_idx = pick_best_index(query, &foods);
    let best = foods[best_idx].clone();

    let mut current_detail: Option<DetailFood> =
        match fetch_detail(&http, &token, &best.food_id).await {
            Ok(d) => Some(d),
            Err(e) => {
                warn!(food_id = %best.food_id, error = %e, "nutrition detail fetch failed");
                None
            }
        };

    let mut current_mode = DisplayMode::Fast;
    let mut current_id = best.food_id.clone();

    let base = format!("nutrition_{}", ctx.id());

    let initial_embed = match &current_detail {
        Some(d) => build_mode_embed(d, current_mode),
        None => build_search_fallback_embed(&best),
    };
    let initial_components = build_components(
        &base,
        current_mode,
        current_detail.is_some(),
        &foods,
        &current_id,
    );
    let has_components = !initial_components.is_empty();

    let reply = ctx
        .send(
            poise::CreateReply::default()
                .embed(initial_embed)
                .components(initial_components)
                .ephemeral(reply_ephemeral),
        )
        .await?;

    {
        let __duration = __start.elapsed().as_millis() as u64;
        let __db = ctx.data().database.clone();
        let __uid = ctx.author().id.get();
        tokio::spawn(async move {
            if let Err(e) = __db
                .log_command_execution("nutrition", __uid, __duration)
                .await
            {
                tracing::error!(error = %e, "failed to log nutrition analytics");
            }
        });
    }

    if !has_components {
        return Ok(());
    }

    let msg = reply.message().await?;

    drive_collector(
        ctx.serenity_context(),
        &http,
        &base,
        msg.id,
        ctx.author().id,
        &foods,
        &mut current_detail,
        &mut current_mode,
        &mut current_id,
    )
    .await;

    let final_embed = match &current_detail {
        Some(d) => build_mode_embed(d, current_mode),
        None => foods
            .iter()
            .find(|f| f.food_id == current_id)
            .map(build_search_fallback_embed)
            .unwrap_or_else(|| build_search_fallback_embed(&best)),
    };
    let _ = reply
        .edit(
            ctx,
            poise::CreateReply::default()
                .embed(final_embed)
                .components(Vec::<CreateActionRow>::new()),
        )
        .await;

    Ok(())
}

/// Drive the Fast/Full + alternatives component loop for a nutrition message
/// until the collector times out, mutating `current_*` in place so the caller can
/// render a final, component-free snapshot. Shared by the owner's reply and the
/// ephemeral copies handed to other users.
#[allow(clippy::too_many_arguments)]
async fn drive_collector(
    serenity_ctx: &serenity::Context,
    http: &reqwest::Client,
    base: &str,
    message_id: MessageId,
    author_id: UserId,
    foods: &[SearchFood],
    current_detail: &mut Option<DetailFood>,
    current_mode: &mut DisplayMode,
    current_id: &mut String,
) {
    let mut stream = serenity::ComponentInteractionCollector::new(serenity_ctx)
        .message_id(message_id)
        .author_id(author_id)
        .timeout(COLLECTOR_TIMEOUT)
        .stream();

    while let Some(mci) = stream.next().await {
        let cid = mci.data.custom_id.as_str();

        if cid.ends_with("_fast") || cid.ends_with("_full") {
            let Some(detail) = current_detail.as_ref() else {
                continue;
            };
            *current_mode = if cid.ends_with("_fast") {
                DisplayMode::Fast
            } else {
                DisplayMode::Full
            };
            let new_embed = build_mode_embed(detail, *current_mode);
            let new_components =
                build_components(base, *current_mode, true, foods, current_id.as_str());

            let _ = mci
                .create_response(
                    serenity_ctx,
                    CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .embed(new_embed)
                            .components(new_components),
                    ),
                )
                .await;
        } else if cid.ends_with("_select") {
            let selected_id = match &mci.data.kind {
                ComponentInteractionDataKind::StringSelect { values } => values.first().cloned(),
                _ => None,
            };
            let Some(selected_id) = selected_id else {
                continue;
            };

            let token = match get_access_token(http).await {
                Ok(t) => t,
                Err(e) => {
                    warn!(error = %e, "nutrition token refresh failed");
                    let _ = mci
                        .create_response(
                            serenity_ctx,
                            CreateInteractionResponse::Message(
                                CreateInteractionResponseMessage::new()
                                    .content("Could not refresh authentication.")
                                    .ephemeral(true),
                            ),
                        )
                        .await;
                    continue;
                }
            };

            let (new_embed, has_detail) = match fetch_detail(http, &token, &selected_id).await {
                Ok(d) => {
                    let embed = build_mode_embed(&d, *current_mode);
                    *current_detail = Some(d);
                    (embed, true)
                }
                Err(e) => {
                    warn!(food_id = %selected_id, error = %e, "nutrition detail fetch failed");
                    let Some(f) = foods.iter().find(|f| f.food_id == selected_id) else {
                        let _ = mci
                            .create_response(
                                serenity_ctx,
                                CreateInteractionResponse::Message(
                                    CreateInteractionResponseMessage::new()
                                        .content("Could not load nutrition for that item.")
                                        .ephemeral(true),
                                ),
                            )
                            .await;
                        continue;
                    };
                    *current_detail = None;
                    (build_search_fallback_embed(f), false)
                }
            };

            *current_id = selected_id;
            let new_components =
                build_components(base, *current_mode, has_detail, foods, current_id.as_str());

            let _ = mci
                .create_response(
                    serenity_ctx,
                    CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .embed(new_embed)
                            .components(new_components),
                    ),
                )
                .await;
        }
    }
}

/// The user who created the message a component is attached to: the original
/// slash-command invoker for a public reply, or the recipient of an ephemeral
/// copy. Read from `interaction_metadata`, falling back to the legacy field.
fn interaction_owner(msg: &Message) -> Option<UserId> {
    if let Some(meta) = msg.interaction_metadata.as_deref()
        && let Some(id) = metadata_user_id(meta)
    {
        return Some(id);
    }
    #[allow(deprecated)]
    msg.interaction.as_ref().map(|i| i.user.id)
}

fn metadata_user_id(meta: &MessageInteractionMetadata) -> Option<UserId> {
    match meta {
        MessageInteractionMetadata::Command(m) => Some(m.user.id),
        MessageInteractionMetadata::Component(m) => Some(m.user.id),
        MessageInteractionMetadata::ModalSubmit(m) => Some(m.user.id),
        _ => None,
    }
}

/// Which food (and starting mode) an ephemeral copy should open on, parsed from
/// the component the foreign user clicked. Fast/Full buttons embed the food id;
/// the alternatives menu carries the chosen id as its selected value.
fn resolve_seed(cid: &str, kind: &ComponentInteractionDataKind) -> Option<(String, DisplayMode)> {
    if cid.ends_with("_select") {
        match kind {
            ComponentInteractionDataKind::StringSelect { values } => {
                values.first().cloned().map(|id| (id, DisplayMode::Fast))
            }
            _ => None,
        }
    } else if let Some(rest) = cid.strip_suffix("_fast") {
        rest.rsplit('_')
            .next()
            .map(|id| (id.to_string(), DisplayMode::Fast))
    } else if let Some(rest) = cid.strip_suffix("_full") {
        rest.rsplit('_')
            .next()
            .map(|id| (id.to_string(), DisplayMode::Full))
    } else {
        None
    }
}

/// Global handler for component clicks on a nutrition message. The per-invocation
/// collector only serves the original invoker; for anyone else we ack the click
/// and hand them their own ephemeral, fully interactive copy seeded to the food
/// they were looking at, instead of letting Discord reject the interaction.
/// Returns true if the interaction was claimed.
pub async fn handle_foreign_component(
    serenity_ctx: &serenity::Context,
    mci: &ComponentInteraction,
) -> bool {
    let cid = mci.data.custom_id.as_str();
    if !cid.starts_with("nutrition_") {
        return false;
    }

    match interaction_owner(&mci.message) {
        Some(owner) if owner == mci.user.id => return false,
        Some(_) => {}
        None => return false,
    }

    let Some((seed_food_id, seed_mode)) = resolve_seed(cid, &mci.data.kind) else {
        return false;
    };

    if mci
        .create_response(
            serenity_ctx,
            CreateInteractionResponse::Defer(
                CreateInteractionResponseMessage::new().ephemeral(true),
            ),
        )
        .await
        .is_err()
    {
        return true;
    }

    let serenity_ctx = serenity_ctx.clone();
    let mci = mci.clone();
    tokio::spawn(async move {
        if let Err(e) = run_foreign_copy(&serenity_ctx, &mci, seed_food_id, seed_mode).await {
            warn!(error = %e, "nutrition ephemeral copy failed");
            let _ = mci
                .edit_response(
                    &serenity_ctx,
                    EditInteractionResponse::new()
                        .content("Could not open a nutrition view for that food."),
                )
                .await;
        }
    });

    true
}

/// Build and drive an ephemeral, interactive nutrition view for a user who
/// clicked on someone else's message. Seeds on the exact food they touched and
/// re-runs the search by name so the copy offers the same "other matches" menu.
async fn run_foreign_copy(
    serenity_ctx: &serenity::Context,
    mci: &ComponentInteraction,
    seed_food_id: String,
    seed_mode: DisplayMode,
) -> Result<(), Error> {
    let http = crate::utils::net::shared_client().clone();
    let token = get_access_token(&http).await?;

    let detail = fetch_detail(&http, &token, &seed_food_id).await?;

    let foods = search_foods(&http, &token, &detail.food_name)
        .await
        .unwrap_or_default();

    let base = format!("nutrition_{}", mci.id.get());
    let mut current_id = seed_food_id;
    let mut current_mode = seed_mode;

    let initial_embed = build_mode_embed(&detail, current_mode);
    let initial_components = build_components(&base, current_mode, true, &foods, &current_id);
    let mut current_detail = Some(detail);

    mci.edit_response(
        serenity_ctx,
        EditInteractionResponse::new()
            .embed(initial_embed)
            .components(initial_components),
    )
    .await?;

    let msg = mci.get_response(serenity_ctx).await?;

    drive_collector(
        serenity_ctx,
        &http,
        &base,
        msg.id,
        mci.user.id,
        &foods,
        &mut current_detail,
        &mut current_mode,
        &mut current_id,
    )
    .await;

    let final_embed = current_detail
        .as_ref()
        .map(|d| build_mode_embed(d, current_mode))
        .or_else(|| {
            foods
                .iter()
                .find(|f| f.food_id == current_id)
                .map(build_search_fallback_embed)
        });
    let mut edit = EditInteractionResponse::new().components(Vec::<CreateActionRow>::new());
    if let Some(embed) = final_embed {
        edit = edit.embed(embed);
    }
    let _ = mci.edit_response(serenity_ctx, edit).await;

    Ok(())
}
