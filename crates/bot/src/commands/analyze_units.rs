use crate::data::{Context, Error};
use crate::utils::emojis;
use macros::track_analytics;
use poise::serenity_prelude as serenity;
use regex::Regex;
use serenity::all::{Colour, CreateEmbed, CreateEmbedFooter};
use std::sync::LazyLock;

/// A glucose reading detected in a message.
#[derive(Debug)]
struct Reading {
    /// The matched text as it appeared, par exemple "120 mg/dL" or a bare "145".
    original: String,
    kind: ReadingKind,
}

#[derive(Debug)]
enum ReadingKind {
    /// A reading whose unit is known: labeled explicitly, or confidently
    /// guessed from a bare number. Converted to the other unit.
    Known {
        /// The unit we read it as (for guessed bare numbers this is shown).
        from_unit: &'static str,
        converted_value: f64,
        converted_unit: &'static str,
        guessed: bool,
    },
    /// A bare number in the overlap band (15-30), valid as either unit, so we
    /// show both interpretations rather than pick one.
    Ambiguous {
        /// Value read as mg/dL, converted to mmol/L.
        as_mgdl_to_mmol: f64,
        /// Value read as mmol/L, converted to mg/dL.
        as_mmol_to_mgdl: f64,
    },
}

/// Detection patterns paired with whether a match is mg/dL (`true`) or mmol/L
/// (`false`). The numeric part accepts a `.` or `,` decimal.
static GLUCOSE_PATTERNS: LazyLock<[(Regex, bool); 4]> = LazyLock::new(|| {
    [
        (
            Regex::new(r"(\d+(?:[.,]\d+)?)\s*(?:mg/dl|mg/dL|mgdl|MGDL|MG/DL)").unwrap(),
            true,
        ),
        (
            Regex::new(r"(\d+(?:[.,]\d+)?)\s*(?:mmol/l|mmol/L|mmoll|MMOL/L|MMOLL)").unwrap(),
            false,
        ),
        (Regex::new(r"(\d+(?:[.,]\d+)?)\s*mg\b").unwrap(), true),
        (Regex::new(r"(\d+(?:[.,]\d+)?)\s*mmol\b").unwrap(), false),
    ]
});

/// Any standalone number, with an optional `.` or `,` decimal. Used to find
/// readings written without a unit.
static NUMBER_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b\d+(?:[.,]\d+)?\b").unwrap());

/// Scan a message for blood glucose values and convert them between mg/dL and mmol/L.
#[poise::command(
    context_menu_command = "Analyze Units",
    install_context = "Guild|User",
    interaction_context = "Guild|BotDm|PrivateChannel"
)]
#[track_analytics("analyze_units")]
pub async fn analyze_units(
    ctx: Context<'_>,
    #[description = "Message to scan for glucose values"] message: serenity::Message,
) -> Result<(), Error> {
    let readings = detect_glucose_units(&message.content);

    if readings.is_empty() {
        let embed = CreateEmbed::new()
            .title(format!("{} No Blood Glucose Values Found", emojis::warning()))
            .description(
                "No blood glucose values (mg/dL, mmol/L, or a bare number) were detected in this message.",
            )
            .color(Colour::ORANGE);

        ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
            .await?;
        return Ok(());
    }

    let any_guessed = readings.iter().any(|r| {
        matches!(
            r.kind,
            ReadingKind::Ambiguous { .. } | ReadingKind::Known { guessed: true, .. }
        )
    });

    let conversion_list = readings
        .iter()
        .map(format_reading)
        .collect::<Vec<_>>()
        .join("\n");

    let footer = if any_guessed {
        "Values without units are guessed from their size, so double-check them."
    } else {
        "Conversions detected from the message."
    };

    let embed = CreateEmbed::new()
        .title(format!(
            "{} Blood Glucose Unit Conversions",
            emojis::sugar()
        ))
        .description(format!(
            "Found {} value(s):\n\n{}",
            readings.len(),
            conversion_list
        ))
        .color(Colour::BLUE)
        .footer(CreateEmbedFooter::new(footer));

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;

    Ok(())
}

/// Render one reading as a bullet line for the embed.
fn format_reading(r: &Reading) -> String {
    match &r.kind {
        ReadingKind::Known {
            from_unit,
            converted_value,
            converted_unit,
            guessed,
        } => {
            if *guessed {
                format!(
                    "• **{}** _(guessed {})_ → **{:.1} {}**",
                    r.original, from_unit, converted_value, converted_unit
                )
            } else {
                format!(
                    "• **{}** → **{:.1} {}**",
                    r.original, converted_value, converted_unit
                )
            }
        }
        ReadingKind::Ambiguous {
            as_mgdl_to_mmol,
            as_mmol_to_mgdl,
        } => format!(
            "• **{}** → **{:.1} mmol/L** _(if mg/dL)_ or **{:.1} mg/dL** _(if mmol/L)_",
            r.original, as_mgdl_to_mmol, as_mmol_to_mgdl
        ),
    }
}

/// Parse a number that may use `,` as its decimal separator.
fn parse_num(s: &str) -> f64 {
    s.replace(',', ".").parse().unwrap_or(0.0)
}

/// Build a `Known` reading if the value is physiologically plausible for its
/// unit (mg/dL 20-600, mmol/L 1-35), else `None`.
fn known_reading(original: String, value: f64, is_mgdl: bool, guessed: bool) -> Option<Reading> {
    let (from_unit, converted_value, converted_unit, in_range) = if is_mgdl {
        (
            "mg/dL",
            value / 18.0,
            "mmol/L",
            (20.0..=600.0).contains(&value),
        )
    } else {
        (
            "mmol/L",
            value * 18.0,
            "mg/dL",
            (1.0..=35.0).contains(&value),
        )
    };

    in_range.then_some(Reading {
        original,
        kind: ReadingKind::Known {
            from_unit,
            converted_value,
            converted_unit,
            guessed,
        },
    })
}

/// Find glucose readings in `content` and convert each to the other unit.
///
/// Readings written with a unit are detected first. Any remaining bare number
/// is then guessed: a decimal comma or a value below 15 reads as mmol/L, above
/// 30 as mg/dL, and 15-30 is ambiguous (shown as both). Values outside a sane
/// range are ignored so ordinary numbers aren't mistaken for readings.
fn detect_glucose_units(content: &str) -> Vec<Reading> {
    let mut readings = Vec::new();
    let mut labeled_spans: Vec<(usize, usize)> = Vec::new();

    for (re, is_mgdl) in GLUCOSE_PATTERNS.iter() {
        for cap in re.captures_iter(content) {
            let whole = cap.get(0).unwrap();
            if labeled_spans
                .iter()
                .any(|(s, e)| whole.start() < *e && *s < whole.end())
            {
                continue;
            }
            labeled_spans.push((whole.start(), whole.end()));

            let Some(value_match) = cap.get(1) else {
                continue;
            };
            let value = parse_num(value_match.as_str());
            if let Some(reading) = known_reading(whole.as_str().to_string(), value, *is_mgdl, false)
            {
                readings.push(reading);
            }
        }
    }

    for m in NUMBER_PATTERN.find_iter(content) {
        if labeled_spans
            .iter()
            .any(|(s, e)| m.start() < *e && *s < m.end())
        {
            continue;
        }

        let text = m.as_str();
        let value = parse_num(text);
        let has_comma = text.contains(',');

        let reading = if has_comma || value < 15.0 {
            known_reading(text.to_string(), value, false, true)
        } else if value > 30.0 {
            known_reading(text.to_string(), value, true, true)
        } else {
            Some(Reading {
                original: text.to_string(),
                kind: ReadingKind::Ambiguous {
                    as_mgdl_to_mmol: value / 18.0,
                    as_mmol_to_mgdl: value * 18.0,
                },
            })
        };

        if let Some(reading) = reading {
            readings.push(reading);
        }
    }

    readings.sort_by(|a, b| a.original.cmp(&b.original));
    readings.dedup_by(|a, b| a.original == b.original);

    readings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn originals(content: &str) -> Vec<(String, String)> {
        detect_glucose_units(content)
            .into_iter()
            .map(|r| {
                let line = format_reading(&r);
                (r.original, line)
            })
            .collect()
    }

    #[test]
    fn guesses_bare_numbers_by_magnitude() {
        let content = "went from 6.4 to 145, sometimes 20";
        let found: Vec<String> = detect_glucose_units(content)
            .into_iter()
            .map(|r| match r.kind {
                ReadingKind::Known { from_unit, .. } => format!("{}:{from_unit}", r.original),
                ReadingKind::Ambiguous { .. } => format!("{}:both", r.original),
            })
            .collect();
        assert!(found.contains(&"145:mg/dL".to_string()), "{found:?}");
        assert!(found.contains(&"6.4:mmol/L".to_string()), "{found:?}");
        assert!(found.contains(&"20:both".to_string()), "{found:?}");
    }

    #[test]
    fn comma_reads_as_mmol() {
        let found = originals("I'm at 6,4 today");
        assert_eq!(found.len(), 1);
        assert!(found[0].1.contains("guessed mmol/L"), "{found:?}");
        assert!(found[0].1.contains("115.2 mg/dL"), "{found:?}");
    }

    #[test]
    fn labeled_number_not_double_counted() {
        let readings = detect_glucose_units("145 mg/dL");
        assert_eq!(readings.len(), 1, "{readings:?}");
        assert_eq!(readings[0].original, "145 mg/dL");
    }

    #[test]
    fn ignores_out_of_range_numbers() {
        assert!(detect_glucose_units("in 2024 I had 0 problems").is_empty());
    }
}
