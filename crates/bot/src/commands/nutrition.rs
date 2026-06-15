use crate::data::{Context, Error};
use crate::utils::emojis;
use futures::StreamExt;
use poise::serenity_prelude as serenity;
use serde::Deserialize;
use serenity::all::{
    ButtonStyle, Colour, ComponentInteractionDataKind, CreateActionRow, CreateButton, CreateEmbed,
    CreateEmbedFooter, CreateInteractionResponse, CreateInteractionResponseMessage,
    CreateSelectMenu, CreateSelectMenuKind, CreateSelectMenuOption,
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
        Some(b) => format!("{} {} - {}", emojis::NUTRITION, name, b),
        None => format!("{} {}", emojis::NUTRITION, name),
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
            format!("{} Calories", emojis::NUTRITION),
            fmt_kcal(s.calories.as_deref()),
            true,
        )
        .field(
            format!("{} Carbs", emojis::CARBS),
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
            format!("{} Calories", emojis::NUTRITION),
            fmt_kcal(s.calories.as_deref()),
            true,
        )
        .field(
            format!("{} Carbs", emojis::CARBS),
            fmt_g(s.carbohydrate.as_deref()),
            true,
        )
        .field(
            format!("{} Protein", emojis::PROTEIN),
            fmt_g(s.protein.as_deref()),
            true,
        )
        .field(
            format!("{} Fat", emojis::FAT),
            fmt_g(s.fat.as_deref()),
            true,
        )
        .field(
            format!("{} Sugar", emojis::SUGAR),
            fmt_g(s.sugar.as_deref()),
            true,
        )
        .field(
            format!("{} Fiber", emojis::FIBERS),
            fmt_g(s.fiber.as_deref()),
            true,
        );

    if parse_num(s.saturated_fat.as_deref()).filter(|n| *n > 0.0).is_some() {
        e = e.field(
            format!("{} Sat. Fat", emojis::FAT),
            fmt_g(s.saturated_fat.as_deref()),
            true,
        );
    }
    if parse_num(s.sodium.as_deref()).filter(|n| *n > 0.0).is_some() {
        e = e.field(
            format!("{} Sodium", emojis::SALT),
            fmt_mg(s.sodium.as_deref()),
            true,
        );
    }
    if parse_num(s.cholesterol.as_deref()).filter(|n| *n > 0.0).is_some() {
        e = e.field(
            format!("{} Cholesterol", emojis::FAT),
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

fn build_mode_buttons(base: &str, mode: DisplayMode) -> CreateActionRow {
    CreateActionRow::Buttons(vec![
        CreateButton::new(format!("{base}_fast"))
            .label("Fast Info")
            .style(ButtonStyle::Primary)
            .disabled(mode == DisplayMode::Fast),
        CreateButton::new(format!("{base}_full"))
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
            let mut opt =
                CreateSelectMenuOption::new(label, &f.food_id).description(truncate(&f.food_description, 100));
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
        rows.push(build_mode_buttons(base, mode));
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

    crate::tips::safe_defer(ctx).await?;

    let http = reqwest::Client::new();

    let token = match get_access_token(&http).await {
        Ok(t) => t,
        Err(e) => {
            warn!("[nutrition] failed to get FatSecret token: {}", e);
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
            warn!("[nutrition] search failed: {}", e);
            send_error!(ctx, "Search Failed", "Failed to reach nutrition service.");
            return Ok(());
        }
    };

    debug!("[nutrition] query={:?} results={}", query, foods.len());

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
                warn!("[nutrition] detail fetch failed for {}: {}", best.food_id, e);
                None
            }
        };

    let mut current_mode = DisplayMode::Fast;
    let mut current_id = best.food_id.clone();

    let base = format!("nutrition_{}", ctx.id());
    let id_fast = format!("{base}_fast");
    let id_full = format!("{base}_full");
    let id_select = format!("{base}_select");

    let initial_embed = match &current_detail {
        Some(d) => build_mode_embed(d, current_mode),
        None => build_search_fallback_embed(&best),
    };
    let initial_components = build_components(&base, current_mode, current_detail.is_some(), &foods, &current_id);
    let has_components = !initial_components.is_empty();

    let reply = ctx
        .send(
            poise::CreateReply::default()
                .embed(initial_embed)
                .components(initial_components),
        )
        .await?;

    {
        let __duration = __start.elapsed().as_millis() as u64;
        let __db = ctx.data().database.clone();
        let __uid = ctx.author().id.get();
        tokio::spawn(async move {
            if let Err(e) = __db.log_command_execution("nutrition", __uid, __duration).await {
                tracing::error!("Analytics Error [nutrition]: {}", e);
            }
        });
    }

    if !has_components {
        return Ok(());
    }

    let msg = reply.message().await?;
    let serenity_ctx = ctx.serenity_context().clone();

    let mut stream = serenity::ComponentInteractionCollector::new(&serenity_ctx)
        .message_id(msg.id)
        .author_id(ctx.author().id)
        .timeout(COLLECTOR_TIMEOUT)
        .stream();

    while let Some(mci) = stream.next().await {
        let cid = mci.data.custom_id.as_str();

        if cid == id_fast || cid == id_full {
            let Some(ref detail) = current_detail else {
                continue;
            };
            current_mode = if cid == id_fast {
                DisplayMode::Fast
            } else {
                DisplayMode::Full
            };
            let new_embed = build_mode_embed(detail, current_mode);
            let new_components =
                build_components(&base, current_mode, true, &foods, &current_id);

            let _ = mci
                .create_response(
                    &serenity_ctx,
                    CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .embed(new_embed)
                            .components(new_components),
                    ),
                )
                .await;
        } else if cid == id_select {
            let selected_id = match &mci.data.kind {
                ComponentInteractionDataKind::StringSelect { values } => {
                    values.first().cloned()
                }
                _ => None,
            };
            let Some(selected_id) = selected_id else {
                continue;
            };

            let token = match get_access_token(&http).await {
                Ok(t) => t,
                Err(e) => {
                    warn!("[nutrition] token refresh failed: {}", e);
                    let _ = mci
                        .create_response(
                            &serenity_ctx,
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

            let (new_embed, has_detail) = match fetch_detail(&http, &token, &selected_id).await {
                Ok(d) => {
                    let embed = build_mode_embed(&d, current_mode);
                    current_detail = Some(d);
                    (embed, true)
                }
                Err(e) => {
                    warn!("[nutrition] detail fetch failed for {}: {}", selected_id, e);
                    let fallback = foods.iter().find(|f| f.food_id == selected_id);
                    let Some(f) = fallback else {
                        let _ = mci
                            .create_response(
                                &serenity_ctx,
                                CreateInteractionResponse::Message(
                                    CreateInteractionResponseMessage::new()
                                        .content("Could not load nutrition for that item.")
                                        .ephemeral(true),
                                ),
                            )
                            .await;
                        continue;
                    };
                    current_detail = None;
                    (build_search_fallback_embed(f), false)
                }
            };

            current_id = selected_id;
            let new_components =
                build_components(&base, current_mode, has_detail, &foods, &current_id);

            let _ = mci
                .create_response(
                    &serenity_ctx,
                    CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .embed(new_embed)
                            .components(new_components),
                    ),
                )
                .await;
        }
    }

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
