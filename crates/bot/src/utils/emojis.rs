#![allow(dead_code)]
use std::collections::HashMap;
use std::sync::OnceLock;

/// Which baked-in ID set to use as the fallback. Selected once from `EMOJI_SET`.
#[derive(Clone, Copy)]
enum EmojiSet {
    Prod,
    Beta,
}

fn active_set() -> EmojiSet {
    static SET: OnceLock<EmojiSet> = OnceLock::new();
    *SET.get_or_init(|| {
        let value = std::env::var("EMOJI_SET").unwrap_or_default();
        if value.trim().eq_ignore_ascii_case("beta") {
            EmojiSet::Beta
        } else {
            EmojiSet::Prod
        }
    })
}

/// `name -> "<:name:id>"` for the running application's emojis. Populated once by
/// [`init`]; until then (and for any name not found) accessors fall back to the
/// `EMOJI_SET`-selected baked-in id below.
static RESOLVED: OnceLock<HashMap<String, String>> = OnceLock::new();

/// Populate the emoji map from the running application's own emojis, given
/// `(name, id, animated)` tuples. Call once at startup; later calls are ignored.
pub fn init<I: IntoIterator<Item = (String, u64, bool)>>(emojis: I) {
    let map = emojis
        .into_iter()
        .map(|(name, id, animated)| {
            let prefix = if animated { "a" } else { "" };
            let rendered = format!("<{prefix}:{name}:{id}>");
            (name, rendered)
        })
        .collect();
    let _ = RESOLVED.set(map);
}

/// Resolve an emoji name to the running app's rendering (from the startup fetch),
/// or the `EMOJI_SET`-selected baked-in fallback when the fetch isn't loaded or
/// the name isn't present on this application.
fn resolve(name: &str, prod: &'static str, beta: &'static str) -> &'static str {
    if let Some(map) = RESOLVED.get()
        && let Some(rendered) = map.get(name)
    {
        return rendered.as_str();
    }
    match active_set() {
        EmojiSet::Prod => prod,
        EmojiSet::Beta => beta,
    }
}

/// Defines one accessor per emoji plus [`NAMES`]. Each entry is
/// `fn_name => "emoji_name", "prod_id", "beta_id"` (the two ids match for the
/// emojis that are shared between the bots).
macro_rules! define_emojis {
    ($($fn_name:ident => $name:literal, $prod:literal, $beta:literal);* $(;)?) => {
        $(
            pub fn $fn_name() -> &'static str {
                resolve(
                    $name,
                    concat!("<:", $name, ":", $prod, ">"),
                    concat!("<:", $name, ":", $beta, ">"),
                )
            }
        )*

        /// Every emoji name the bot expects its application to have.
        pub const NAMES: &[&str] = &[$($name),*];
    };
}

define_emojis! {
    nutrition => "nutrition", "1508946829344116798", "1508946829344116798";
    sugar => "sugar", "1508946740735381635", "1508946740735381635";
    fat => "fat", "1508946555778891866", "1508946555778891866";
    protein => "protein", "1508946415886401830", "1508946415886401830";
    salt => "salt", "1508946287700213901", "1508946287700213901";
    fibers => "fibers", "1508945974469591060", "1508945974469591060";
    carbs => "carbs", "1508945724321304637", "1508945724321304637";

    wifi_add => "wifi_add", "1501904931958034572", "1501904931958034572";
    wifi_issue => "wifi_issue", "1501904907672752239", "1501904907672752239";
    wifi_off => "wifi_off", "1501904788491735080", "1501904788491735080";
    wifi => "wifi", "1501904753523818587", "1501904753523818587";
    wifi_locked => "wifi_locked", "1501904709257269338", "1501904709257269338";

    lock_open => "lock_open", "1501904665732710421", "1501904665732710421";
    lock_closed => "lock_closed", "1501904634829344769", "1501904634829344769";
    password => "password", "1501904284084863036", "1501904284084863036";

    water => "water", "1501903359756472461", "1501903359756472461";
    outage => "outage", "1501903060694208582", "1501903060694208582";
    sync_problem => "sync_problem", "1501902960844869766", "1501902960844869766";

    bug_green => "bug_green", "1501902629591187708", "1501902629591187708";
    bug => "bug", "1501902516382859294", "1501902516382859294";

    // The eight below differ between prod and beta (re-uploaded emojis).
    warning => "warning", "1516837238250537101", "1501901984096059503";
    donate => "donate", "1516837200594075661", "1501901511549259826";
    kofi_logo => "kofi_logo", "1501875246599503972", "1501875431375245422";
    date_valid => "date_valid", "1516836999984713919", "1501897963411083325";
    date_invalid => "date_invalid", "1516837071346602085", "1501898105614897312";
    add_user => "add_user", "1516837168352596119", "1501901373959045191";
    remove_user => "remove_user", "1516837183280123944", "1501901363792183467";
    sticker_add => "sticker_add", "1516837139159973969", "1501899215532720138";

    tip => "tip", "1508956089704775830", "1508956089704775830";
    celebration => "celebration", "1508955985044045854", "1508955985044045854";
    error => "error", "1508956486980731101", "1508956486980731101";
    micro_bolus => "micro_bolus", "1508956739188555987", "1508956739188555987";
    image_mode => "image", "1508956922705875015", "1508956922705875015";
}
