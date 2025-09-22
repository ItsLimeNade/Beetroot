use reqwest::{Client, Url};
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug)]
/// Represents a Nightscout client for interacting with the Nightscout API.
/// 
/// This struct holds an HTTP client used to send requests to the Nightscout service.
pub struct Nightscout {
    http_client: Client
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
#[derive(Deserialize, Debug, Clone)]
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

#[derive(Debug, Default, Clone, Copy)]
pub struct NightscoutRequestOptions {
    pub count: Option<u8>
}

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

impl Nightscout {
    /// Creates a new instance of `Nightscout` with a default HTTP client.
    pub fn new() -> Self {
        Nightscout { http_client: Client::new() }
    }

    /// Returns an `Entry` if available, or a `NightscoutError::NoEntries` if no entries are found.
    pub async fn get_entry(&self, base_url: &str) -> Result<Entry, NightscoutError> {
        let entries = self.get_entries(base_url, NightscoutRequestOptions::default()).await?;
        entries.first().cloned().ok_or(NightscoutError::NoEntries)
    }

    /// The number of entries returned is determined by `options.count`, defaulting to 1 if not specified.
    /// Returns a vector of `Entry` objects or a `NightscoutError` on failure.
    pub async fn get_entries(&self, base_url: &str, options: NightscoutRequestOptions) -> Result<Vec<Entry>, NightscoutError> {
        let count: u8 = options.count.unwrap_or(1);

        let base = Url::parse(base_url)?;
        let url = base.join(&format!("api/v1/entries/sgv?count={count}"))?;
        
        let req = self.http_client.get(url);
        let res = req.send().await?.error_for_status()?;

        let entries: Vec<Entry>  = res.json().await?;

        Ok(entries)
    }
}
