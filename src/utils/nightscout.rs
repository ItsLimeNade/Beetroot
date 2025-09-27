use chrono::{Duration, Local, TimeZone, Utc};
use chrono_tz::Tz;
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
    #[error("Missing data in the response body.")]
    MissingData,
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
    pub date_string: Option<String>,
    #[serde(default)]
    pub date: Option<u64>,
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
#[derive(Clone, Copy, Debug)]
pub struct Delta {
    pub value: f32,
}

#[allow(dead_code)]
impl Delta {
    pub fn as_signed_str(&self) -> String {
        let sign = if self.value >= 0.0 { "+" } else { "" };

        format!("{}{}", &sign, &self.value)
    }

    pub fn as_mmol(&self) -> Self {
        Delta { value: ((self.value/18.) * 10.0).round() / 10.0}
    }
}

#[allow(dead_code)]
impl Entry {

    pub fn svg_as_mmol(&self) -> f32 {
        ((self.sgv/18.) * 10.0).round() / 10.0
    }

    pub fn millis_to_timestamp(&self) -> chrono::DateTime<Local> {
        let timestamp = self.date.or(self.mills);

        if let Some(ms) = timestamp {
            Local
                .timestamp_millis_opt(ms as i64)
                .single()
                .unwrap_or_else(Local::now) 
        } else if let Some(date_str) = &self.date_string {
            match chrono::DateTime::parse_from_rfc3339(date_str) {
                Ok(parsed) => parsed.with_timezone(&Local),
                Err(_) => Local::now(),
            }
        } else {
            Local::now()
        }
    }
    pub fn millis_to_user_timezone(&self, user_timezone: &str) -> chrono::DateTime<chrono_tz::Tz> {
        let tz: Tz = user_timezone.parse().unwrap_or(chrono_tz::UTC);
        let timestamp = self.date.or(self.mills);

        if let Some(ms) = timestamp {
            if let Some(utc_dt) = chrono::DateTime::from_timestamp_millis(ms as i64) {
                utc_dt.with_timezone(&tz)
            } else {
                chrono::Utc::now().with_timezone(&tz)
            }
        } else if let Some(date_str) = &self.date_string {
            match chrono::DateTime::parse_from_rfc3339(date_str) {
                Ok(parsed) => parsed.with_timezone(&tz),
                Err(_) => chrono::Utc::now().with_timezone(&tz),
            }
        } else {
            chrono::Utc::now().with_timezone(&tz)
        }
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
    /// assert_eq!(entry.trend(), Trend::Flat);
    /// ```
    ///
    pub fn trend(&self) -> Trend {
        if let Some(trend) = &self.direction {
            return Trend::from(trend.as_str());
        }
        Trend::Else
    }

    /// Calculates a delta using two different readings.
    pub fn get_delta(&self, old_entry: &Entry) -> Delta {
        let delta_value = self.sgv - old_entry.sgv;
        Delta { value: delta_value }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct ProfileStore {
    pub timezone: String,
    pub units: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Profile {
    #[serde(rename = "defaultProfile")]
    pub default_profile: String,
    pub store: std::collections::HashMap<String, ProfileStore>,
}

#[allow(dead_code)]
#[derive(Debug, Default, Clone, Copy)]
pub struct NightscoutRequestOptions {
    pub count: Option<u8>,
    pub hours_back: Option<u8>,
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

    /// Sets the number of hours back from now to fetch entries.
    /// This will fetch all entries from the past X hours.
    ///
    /// ```
    /// let options = NightscoutRequestOptions::default()
    /// .hours_back(6); // Fetch entries from the past 6 hours
    /// ```
    pub fn hours_back(mut self, hours: u8) -> Self {
        self.hours_back = Some(hours);
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

    pub async fn get_profile(&self, base_url: &str) -> Result<Profile, NightscoutError> {
        let base = Url::parse(base_url)?;
        let url = base.join("api/v1/profile.json")?;

        let req = self.http_client.get(url);
        let res = req.send().await?.error_for_status()?;
        let profiles: Vec<Profile> = res.json().await?;

        profiles.first().cloned().ok_or(NightscoutError::NoEntries)
    }

    /// Returns an `Entry` if available, or a `NightscoutError::NoEntries` if no entries are found.
    pub async fn get_entry(&self, base_url: &str) -> Result<Entry, NightscoutError> {
        let entries = self
            .get_entries(base_url, NightscoutRequestOptions::default())
            .await?;
        entries.first().cloned().ok_or(NightscoutError::NoEntries)
    }

    /// Fetches entries from Nightscout based on the provided options.
    ///
    /// If `options.hours_back` is set, it will fetch all entries from the past X hours.
    /// If `options.count` is set, it will limit the number of entries returned.
    /// If both are set, `hours_back` takes precedence for the query, but `count` can still limit results.
    ///
    /// The number of entries returned is determined by `options.count`, defaulting to 1 if not specified
    /// and `hours_back` is not used.
    /// Returns a vector of `Entry` objects or a `NightscoutError` on failure.
    pub async fn get_entries(
        &self,
        base_url: &str,
        options: NightscoutRequestOptions,
    ) -> Result<Vec<Entry>, NightscoutError> {
        let base = Url::parse(base_url)?;

        let url = if let Some(hours) = options.hours_back {
            let count = options.count.unwrap_or(u8::MAX);
            let now = Utc::now();
            let hours_ago = now - Duration::hours(hours as i64);
            let start_timestamp = hours_ago.timestamp_millis() as u64;
            let end_timestamp = now.timestamp_millis() as u64;

            let mut query_params = format!(
                "api/v1/entries/sgv.json?find[date][$gte]={}&find[date][$lte]={}",
                start_timestamp, end_timestamp
            );

            query_params.push_str(&format!("&count={}", count));

            base.join(&query_params)?
        } else {
            let count = options.count.unwrap_or(u8::MAX);
            base.join(&format!("api/v1/entries/sgv.json?count={count}"))?
        };
        println!("{}", url);
        let req = self.http_client.get(url);
        let res = req.send().await?.error_for_status()?;
        println!("{:#?}", res);
        let entries: Vec<Entry> = res.json().await?;

        self.clean_entries(&entries)
    }

    /// Convenience method to fetch entries from the past X hours.
    /// This is equivalent to using `get_entries` with `NightscoutRequestOptions::default().hours_back(hours)`.
    ///
    /// # Arguments
    /// * `base_url` - The base URL of your Nightscout instance
    /// * `hours` - Number of hours back from now to fetch entries
    ///
    /// # Returns
    /// * `Ok(Vec<Entry>)` - Vector of entries from the specified time range
    /// * `Err(NightscoutError)` - If the request fails or no entries are found
    ///
    /// # Example
    /// ```
    /// let client = Nightscout::new();
    /// let entries = client.get_entries_for_hours("https://your-nightscout.herokuapp.com", 6).await?;
    /// println!("Found {} entries from the past 6 hours", entries.len());
    /// ```
    pub async fn get_entries_for_hours(
        &self,
        base_url: &str,
        hours: u8,
    ) -> Result<Vec<Entry>, NightscoutError> {
        let options = NightscoutRequestOptions::default().hours_back(hours);
        self.get_entries(base_url, options).await
    }

    /// Gets the ID of the entry's date string
    ///
    /// Example of a date string `2025-09-23T08:38:01.546Z`
    ///
    /// Example of a date string ID `546Z`
    pub fn get_date_id(entry: &Entry) -> Result<&str, NightscoutError> {
        entry
            .date_string
            .as_deref()
            .ok_or(NightscoutError::MissingData)?
            .rsplit_once('.')
            .map(|(_, id)| id)
            .ok_or(NightscoutError::MissingData)
    }

    /// Filters entries to only include those with the same date string ID as the first entry
    ///
    /// Takes a slice of entries and returns a new vector containing only the entries
    /// that have the same date string ID (the part after the last dot in the date string)
    /// as the first entry in the input slice.
    ///
    /// # Arguments
    /// * `entries` - A slice of Entry objects to filter
    ///
    /// # Returns
    /// * `Ok(Vec<Entry>)` - Vector of entries with matching date string IDs
    /// * `Err(NightscoutError::NoEntries)` - If the input slice is empty
    /// * `Err(NightscoutError::MissingData)` - If the first entry lacks a valid date string
    ///
    /// # Example
    /// Given entries with date strings:
    /// - `2025-09-23T08:38:01.546Z` (ID: `546Z`)
    /// - `2025-09-23T08:38:01.546Z` (ID: `546Z`) ← included
    /// - `2025-09-23T08:38:01.789Z` (ID: `789Z`) ← excluded
    ///
    /// Only entries with ID `546Z` would be returned.
    pub fn clean_entries(&self, entries: &[Entry]) -> Result<Vec<Entry>, NightscoutError> {
        if entries.is_empty() {
            return Err(NightscoutError::NoEntries);
        }

        let mut cleaned: Vec<Entry> = Vec::new();
        
        for entry in entries {
            let is_duplicate = cleaned.iter().any(|existing| {
                let entry_timestamp = entry.date.or(entry.mills).unwrap_or(0);
                let existing_timestamp = existing.date.or(existing.mills).unwrap_or(0);
                
                // We consider the entries are duplicate only if:
                // 1) Same SGV value
                // 2) Timestamps within 5 seconds of each other
                let timestamp_diff = (entry_timestamp as i64 - existing_timestamp as i64).abs();
                let same_sgv = (entry.sgv - existing.sgv).abs() < 0.1;
                let close_timestamps = timestamp_diff <= 5000;
                
                same_sgv && close_timestamps
            });
            
            if !is_duplicate {
                cleaned.push(entry.clone());
            }
        }
        
        if cleaned.is_empty() {
            Err(NightscoutError::NoEntries)
        } else {
            Ok(cleaned)
        }
    }

    pub async fn get_current_delta(&self, base_url: &str) -> Result<Delta, NightscoutError> {
        //? Since clean entries could delete some entries due to the duplication glitch, it is
        //? safer to pull more than two. A check to verify that enough entries are available
        //? is also mandatory to avoid stupid errors.
        let options = NightscoutRequestOptions::default().count(4);
        let entries = self.get_entries(base_url, options).await?;
        println!("{}", entries.len());
        if entries.len() < 2 {
            return Err(NightscoutError::NoEntries);
        }

        let newer = &entries[0];
        let older = &entries[1];
        Ok(newer.get_delta(older))
    }
}
