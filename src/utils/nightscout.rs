use reqwest::{Client, Url};
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug)]
pub struct Nightscout {
    http_client: Client
}

#[derive(Debug, Error)]
pub enum NightscoutError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("No entries found")]
    NoEntries,
    #[error("Invalid URL: {0}")]
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
    pub fn count(mut self, count: u8) -> Self {
        self.count = Some(count);
        self
    }
}

impl Nightscout {
    pub fn new() -> Self {
        Nightscout { http_client: Client::new() }
    }

    pub async fn get_entry(&self, base_url: &str) -> Result<Entry, NightscoutError> {
        let entries = self.get_entries(base_url, NightscoutRequestOptions::default()).await?;
        entries.first().cloned().ok_or(NightscoutError::NoEntries)
    }

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
