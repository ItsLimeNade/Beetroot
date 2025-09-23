use reqwest::{Client, Url};
use serde::Deserialize;
use std::convert::From;
use thiserror::Error;

#[derive(Debug)]
/// Represents a Nightscout client for interacting with the Nightscout API.
///
/// This struct holds an HTTP client used to send requests to the Nightscout service.
pub struct Nightscout {
    http_client: Client,
}

#[derive(Debug, Error)]
pub enum NightscoutError {
    #[error("Network error: {0}")]
    /// Represents errors that occur during network requests, such as connection failures,
    /// timeouts, or invalid responses, as reported by the `reqwest` HTTP client library.
    Network(#[from] reqwest::Error),
    /// Represents an error that occurs when no reaings are read when fetching Nightscout.
    #[error("No entries found")]
    NoEntries,
    #[error("Invalid URL: {0}")]
    /// Represents an error that occurs when parsing a URL using the `url` crate.
    ///
    /// This variant is generated when a URL string cannot be successfully parsed.
    /// See [`url::ParseError`](https://docs.rs/url/latest/url/enum.ParseError.html) for more details.
    Url(#[from] url::ParseError),
}

#[allow(dead_code)]
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    #[serde(rename = "_id")]
    id: String,
    pub sgv: f32,
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub delta: Option<f32>,
    #[serde(default)]
    pub date: Option<u64>,
    #[serde(default)]
    pub date_string: Option<String>,
    #[serde(default)]
    pub mills: Option<u64>,
}
#[allow(dead_code)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Trend {
    DoubleUp,
    SingleUp,
    FortyFiveUp,
    Flat,
    FortyFiveDown,
    SingleDown,
    DoubleDown,
    Else,
}

#[allow(dead_code)]
impl Trend {
    pub fn as_arrow(&self) -> &str {
        match self {
            Self::DoubleUp => "↑↑",
            Self::SingleUp => "↑",
            Self::FortyFiveUp => "↗",
            Self::Flat => "→",
            Self::FortyFiveDown => "↘",
            Self::SingleDown => "↓",
            Self::DoubleDown => "↓↓",
            Self::Else => "↮",
        }
    }
}

//? Tried implementing Into For &str, but apparently rust's std automatically implements `Into` when creating
//? a `From`. So no need to explicitely write it otherwise it will create a conflict with the compiler's
//? automatic implementation.
impl From<&str> for Trend {
    /// Converts a slice into a Trend.
    ///
    /// If the slice isn't one of the default values the Trend will default to `Trend::Else`.
    fn from(direction: &str) -> Self {
        match direction {
            "DoubleUp" => Self::DoubleUp,
            "SingleUp" => Self::SingleUp,
            "FortyFiveUp" => Self::FortyFiveUp,
            "Flat" => Self::Flat,
            "FortyFiveDown" => Self::FortyFiveDown,
            "SingleDown" => Self::SingleDown,
            "DoubleDown" => Self::DoubleDown,
            //? I was wondering if we should throw an error if string is invalid or we just give no trend?
            _ => Self::Else,
        }
    }
}

#[allow(dead_code)]
impl Entry {
    fn delta(&mut self, delta_value: f32) {
        self.delta = Some(delta_value);
    }

    /// Converts the Nightscout trend text into a Trend enum.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::utils::nightscout::{Entry, Trend};
    ///
    /// // Create an entry with Flat direction
    /// let entry_json = r#"{"_id": "test", "sgv": 120.0, "direction": "Flat"}"#;
    /// let entry: Entry = serde_json::from_str(entry_json).unwrap();
    ///
    /// assert_eq!(entry.get_trend(), Trend::Flat);
    /// ```
    ///
    pub fn trend(&self) -> Trend {
        if let Some(trend) = &self.direction {
            return Trend::from(trend.as_str());
        }
        Trend::Else
    }
}

#[allow(dead_code)]
#[derive(Debug, Default, Clone, Copy)]
pub struct NightscoutRequestOptions {
    pub count: Option<u8>,
}

#[allow(dead_code)]
impl NightscoutRequestOptions {
    /// Sets the ammount of entries that will be fetched from Nightscout.
    ///
    /// ```
    /// let options = NightscoutRequestOptions::default()
    /// .count(5);
    /// ```
    pub fn count(mut self, count: u8) -> Self {
        self.count = Some(count);
        self
    }
}

#[allow(dead_code)]
impl Nightscout {
    /// Creates a new instance of `Nightscout` with a default HTTP client.
    pub fn new() -> Self {
        Nightscout {
            http_client: Client::new(),
        }
    }

    /// Returns an `Entry` if available, or a `NightscoutError::NoEntries` if no entries are found.
    pub async fn get_entry(&self, base_url: &str) -> Result<Entry, NightscoutError> {
        let entries = self
            .get_entries(base_url, NightscoutRequestOptions::default())
            .await?;
        entries.first().cloned().ok_or(NightscoutError::NoEntries)
    }

    /// The number of entries returned is determined by `options.count`, defaulting to 1 if not specified.
    /// Returns a vector of `Entry` objects or a `NightscoutError` on failure.
    pub async fn get_entries(
        &self,
        base_url: &str,
        options: NightscoutRequestOptions,
    ) -> Result<Vec<Entry>, NightscoutError> {
        let count: u8 = options.count.unwrap_or(1);

        let base = Url::parse(base_url)?;
        let url = base.join(&format!("api/v1/entries/sgv?count={count}"))?;

        let req = self.http_client.get(url);
        let res = req.send().await?.error_for_status()?;

        let entries: Vec<Entry> = res.json().await?;

        Ok(entries)
    }
}
