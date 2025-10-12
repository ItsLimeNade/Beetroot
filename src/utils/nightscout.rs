use chrono::{Duration, Local, TimeZone, Utc};
use chrono_tz::Tz;
use reqwest::{Client, Url};
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Clone)]
pub enum AuthMethod {
    ApiSecret(String),
    Bearer(String),
}

impl AuthMethod {
    pub fn from_token(token: &str) -> Self {
        if token.starts_with("eyJ") {
            Self::Bearer(token.to_string())
        } else {
            Self::ApiSecret(token.to_string())
        }
    }

    /// Convert an access token to JWT using the Nightscout API
    #[allow(dead_code)]
    pub async fn to_jwt(
        nightscout: &Nightscout,
        base_url: &str,
        access_token: &str,
    ) -> Result<Self, NightscoutError> {
        let jwt_response = nightscout.request_jwt_token(base_url, access_token).await?;
        Ok(Self::Bearer(jwt_response.token))
    }

    /// Apply the authentication method to an HTTP request
    pub fn apply_to_request(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self {
            Self::ApiSecret(secret) => {
                tracing::debug!("[AUTH] Using API-SECRET header authentication");
                req.header("API-SECRET", secret)
            }
            Self::Bearer(token) => {
                tracing::debug!("[AUTH] Using Bearer token authentication");
                req.header("Authorization", format!("Bearer {}", token))
            }
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::ApiSecret(_) => "API-SECRET header",
            Self::Bearer(_) => "Bearer token",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct JwtResponse {
    pub token: String,
    pub exp: i64,
}

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
    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),
}

#[allow(dead_code)]
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    #[serde(rename = "_id", default)]
    pub id: Option<String>,
    #[serde(default)]
    pub sgv: f32,
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default, rename = "type")]
    pub entry_type: Option<String>,
    // Handle both possible field names for date string
    #[serde(default, alias = "dateString")]
    pub date_string: Option<String>,
    #[serde(default)]
    pub date: Option<u64>,
    #[serde(default)]
    pub mills: Option<u64>,
    // Meter blood glucose (finger stick reading)
    #[serde(default, deserialize_with = "deserialize_mbg", alias = "MBG")]
    pub mbg: Option<f32>,
}

// Custom deserializer for glucose field that can handle both numbers and strings
fn deserialize_glucose<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Visitor;

    struct GlucoseVisitor;

    impl<'de> Visitor<'de> for GlucoseVisitor {
        type Value = Option<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string, number, or null")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserializer.deserialize_any(GlucoseValueVisitor)
        }
    }

    struct GlucoseValueVisitor;

    impl<'de> Visitor<'de> for GlucoseValueVisitor {
        type Value = Option<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or number")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
            Ok(Some(value.to_string()))
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
            Ok(Some(value.to_string()))
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
            Ok(Some(value.to_string()))
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E> {
            Ok(Some(value.to_string()))
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
    }

    deserializer.deserialize_option(GlucoseVisitor)
}

// Custom deserializer for numeric fields that can handle numbers, strings, or null
fn deserialize_numeric_field<'de, D>(deserializer: D) -> Result<Option<f32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Visitor;

    struct NumericFieldVisitor;

    impl<'de> Visitor<'de> for NumericFieldVisitor {
        type Value = Option<f32>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a number, string, or null")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserializer.deserialize_any(NumericValueVisitor)
        }
    }

    struct NumericValueVisitor;

    impl<'de> Visitor<'de> for NumericValueVisitor {
        type Value = Option<f32>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a number or string")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value.parse::<f32>().ok())
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
            Ok(Some(value as f32))
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
            Ok(Some(value as f32))
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E> {
            Ok(Some(value as f32))
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
    }

    deserializer.deserialize_option(NumericFieldVisitor)
}

// Alias for mbg field deserialization
fn deserialize_mbg<'de, D>(deserializer: D) -> Result<Option<f32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserialize_numeric_field(deserializer)
}

#[allow(dead_code)]
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Treatment {
    #[serde(rename = "_id", default)]
    pub id: Option<String>,
    #[serde(rename = "eventType", default)]
    pub event_type: Option<String>,
    #[serde(rename = "created_at", default)]
    pub created_at: Option<String>,
    #[serde(default, deserialize_with = "deserialize_glucose")]
    pub glucose: Option<String>,
    #[serde(default)]
    pub glucose_type: Option<String>,
    #[serde(default)]
    pub carbs: Option<f32>,
    #[serde(default)]
    pub insulin: Option<f32>,
    #[serde(default)]
    pub units: Option<String>,
    #[serde(default)]
    pub date: Option<u64>,
    #[serde(default)]
    pub mills: Option<u64>,
    #[serde(rename = "type", default)]
    pub type_: Option<String>,
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
        Delta {
            value: ((self.value / 18.) * 10.0).round() / 10.0,
        }
    }
}

#[allow(dead_code)]
impl Entry {
    pub fn svg_as_mmol(&self) -> f32 {
        ((self.sgv / 18.) * 10.0).round() / 10.0
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

    /// Check if this entry has a meter blood glucose (finger stick) reading
    pub fn has_mbg(&self) -> bool {
        if let Some(entry_type) = &self.entry_type {
            if entry_type == "mbg" {
                return self.mbg.is_some() && self.mbg.unwrap_or(0.0) > 0.0;
            }
        }
        self.mbg.is_some() && self.mbg.unwrap_or(0.0) > 0.0
    }
}

#[allow(dead_code)]
impl Treatment {
    /// Get timestamp as local DateTime
    pub fn millis_to_timestamp(&self) -> chrono::DateTime<Local> {
        let timestamp = self.date.or(self.mills);

        if let Some(ms) = timestamp {
            Local
                .timestamp_millis_opt(ms as i64)
                .single()
                .unwrap_or_else(Local::now)
        } else if let Some(date_str) = &self.created_at {
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
        } else if let Some(date_str) = &self.created_at {
            match chrono::DateTime::parse_from_rfc3339(date_str) {
                Ok(parsed) => parsed.with_timezone(&tz),
                Err(_) => chrono::Utc::now().with_timezone(&tz),
            }
        } else {
            chrono::Utc::now().with_timezone(&tz)
        }
    }

    pub fn is_insulin(&self) -> bool {
        self.insulin.is_some() && self.insulin.unwrap_or(0.0) > 0.0
    }

    pub fn is_carbs(&self) -> bool {
        self.carbs.is_some() && self.carbs.unwrap_or(0.0) > 0.0
    }

    pub fn is_glucose_reading(&self) -> bool {
        self.glucose.is_some() && self.glucose_type.as_deref() == Some("Finger")
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct ProfileStore {
    pub timezone: String,
    #[serde(default)]
    pub units: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Profile {
    #[serde(rename = "defaultProfile")]
    pub default_profile: String,
    pub store: std::collections::HashMap<String, ProfileStore>,
}

#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub struct DeviceStatus {
    #[serde(default)]
    pub openaps: Option<OpenApsStatus>,
    #[serde(default)]
    #[allow(dead_code)]
    pub pump: Option<PumpStatus>,
}

#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub struct OpenApsStatus {
    #[serde(default)]
    pub iob: Option<IobData>,
    #[serde(default)]
    pub suggested: Option<SuggestedData>,
}

#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub struct IobData {
    #[serde(default)]
    pub iob: Option<f32>,
    #[serde(default)]
    #[allow(dead_code)]
    pub basaliob: Option<f32>,
    #[serde(default)]
    #[allow(dead_code)]
    pub bolusiob: Option<f32>,
}

#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub struct SuggestedData {
    #[serde(rename = "COB", default)]
    pub cob: Option<f32>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PumpStatus {
    #[serde(default)]
    #[allow(dead_code)]
    pub iob: Option<PumpIob>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PumpIob {
    #[serde(default)]
    #[allow(dead_code)]
    pub bolusiob: Option<f32>,
}

// Alias for cob field deserialization
fn deserialize_cob<'de, D>(deserializer: D) -> Result<Option<f32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserialize_numeric_field(deserializer)
}

#[derive(Deserialize, Debug, Clone)]
pub struct PebbleResponse {
    #[serde(default)]
    pub bgs: Vec<PebbleData>,
}

#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub struct PebbleData {
    #[serde(default)]
    pub sgv: Option<String>,
    #[serde(default)]
    pub trend: Option<i32>,
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub datetime: Option<u64>,
    #[serde(default, deserialize_with = "deserialize_numeric_field")]
    pub bgdelta: Option<f32>,
    #[serde(default)]
    pub battery: Option<String>,
    #[serde(default)]
    pub iob: Option<String>,
    #[serde(default, deserialize_with = "deserialize_cob")]
    pub cob: Option<f32>,
}

#[allow(dead_code)]
#[derive(Debug, Default, Clone, Copy)]
pub struct NightscoutRequestOptions {
    pub count: Option<u16>,
    pub hours_back: Option<u16>,
}

#[allow(dead_code)]
impl NightscoutRequestOptions {
    /// Sets the ammount of entries that will be fetched from Nightscout.
    ///
    /// ```
    /// let options = NightscoutRequestOptions::default()
    /// .count(5);
    /// ```
    pub fn count(mut self, count: u16) -> Self {
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
    pub fn hours_back(mut self, hours: u16) -> Self {
        self.hours_back = Some(hours);
        self
    }
}

#[allow(dead_code)]
impl Nightscout {
    /// Creates a new instance of `Nightscout` with a robust HTTP client.
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(10))
            .danger_accept_invalid_certs(false) // Keep SSL verification enabled
            .build()
            .unwrap_or_else(|e| {
                tracing::warn!("[HTTP] Failed to build custom client, using default: {}", e);
                Client::new()
            });

        Nightscout {
            http_client: client,
        }
    }

    /// Parse and validate a base URL
    fn parse_base_url(base_url: &str) -> Result<Url, NightscoutError> {
        if base_url.trim().is_empty() {
            return Err(NightscoutError::Url(url::ParseError::EmptyHost));
        }

        let url = Url::parse(base_url.trim())?;
        if url.host().is_none() {
            return Err(NightscoutError::Url(url::ParseError::EmptyHost));
        }
        if !matches!(url.scheme(), "http" | "https") {
            return Err(NightscoutError::Url(url::ParseError::InvalidIpv4Address));
        }
        Ok(url)
    }

    /// Handle SSL/connection errors with detailed logging
    fn handle_connection_error(e: reqwest::Error, url: &Url) -> NightscoutError {
        tracing::error!("[ERROR] HTTP request failed: {}", e);
        tracing::error!(
            "[DEBUG] Request details - URL: {}, Error type: {:?}",
            url,
            e
        );

        if e.is_timeout() {
            tracing::error!("[SSL] Connection timeout - check if Nightscout site is accessible");
        } else if e.is_connect() {
            tracing::error!("[SSL] Connection failed - possible SSL certificate or network issue");
            tracing::error!(
                "[SSL] Try accessing {} in a browser to verify SSL certificate",
                url
            );
        } else if e.to_string().contains("certificate")
            || e.to_string().contains("tls")
            || e.to_string().contains("ssl")
        {
            tracing::error!("[SSL] SSL/TLS certificate error detected");
            tracing::error!(
                "[SSL] Your Nightscout site may have an invalid or expired SSL certificate"
            );
            tracing::error!(
                "[SSL] Contact your Nightscout hosting provider to fix the SSL certificate"
            );
        }

        NightscoutError::Network(e)
    }

    /// Request a JWT token from Nightscout using an access token
    pub async fn request_jwt_token(
        &self,
        base_url: &str,
        access_token: &str,
    ) -> Result<JwtResponse, NightscoutError> {
        tracing::debug!("[JWT] Requesting JWT token from Nightscout");

        let base = Url::parse(base_url.trim())?;
        let url = base.join(&format!("api/v2/authorization/request/{}", access_token))?;

        tracing::debug!("[JWT] Request URL: {}", url);

        let res = self.http_client.get(url.clone()).send().await?;

        let res = match res.error_for_status() {
            Ok(response) => {
                tracing::info!("[HTTP] JWT response status: {}", response.status());
                response
            }
            Err(e) => {
                tracing::error!("[ERROR] JWT request failed: {}", e);
                return Err(NightscoutError::Network(e));
            }
        };

        let jwt_response: JwtResponse = res.json().await?;
        tracing::info!(
            "[OK] Successfully obtained JWT token (expires: {})",
            jwt_response.exp
        );

        Ok(jwt_response)
    }

    pub async fn get_profile(
        &self,
        base_url: &str,
        token: Option<&str>,
    ) -> Result<Profile, NightscoutError> {
        tracing::debug!("[API] Fetching profile from URL: '{}'", base_url);
        let auth_method = token.map(AuthMethod::from_token);
        if let Some(ref auth) = auth_method {
            match auth {
                AuthMethod::ApiSecret(secret) => {
                    tracing::debug!(
                        "[AUTH] Using API-SECRET authentication: {}***",
                        &secret[..secret.len().min(8)]
                    );
                }
                AuthMethod::Bearer(jwt) => {
                    tracing::debug!(
                        "[AUTH] Using Bearer JWT authentication: {}***",
                        &jwt[..jwt.len().min(8)]
                    );
                }
            }
        } else {
            tracing::debug!("[AUTH] No authentication token provided");
        }

        let base = Self::parse_base_url(base_url)?;
        tracing::debug!("[OK] Successfully parsed base URL: {}", base);

        let url = base.join("api/v1/profile.json")?;
        tracing::debug!("[API] Profile API URL: {}", url);

        let mut req = self.http_client.get(url.clone());

        if let Some(auth) = auth_method {
            req = auth.apply_to_request(req);
            tracing::debug!("[OK] Applied {} authentication", auth.description());
        }

        tracing::debug!("[HTTP] Sending HTTP request to Nightscout API...");
        let res = match req.send().await {
            Ok(response) => {
                tracing::debug!("[HTTP] Received HTTP response from Nightscout");
                response
            }
            Err(e) => return Err(Self::handle_connection_error(e, &url)),
        };

        let res = match res.error_for_status() {
            Ok(response) => {
                tracing::info!("[HTTP] Profile response status: {}", response.status());
                response
            }
            Err(e) => {
                tracing::error!("[ERROR] Profile request returned error status: {}", e);
                return Err(NightscoutError::Network(e));
            }
        };

        let json: serde_json::Value = res.json().await?;
        tracing::debug!("[JSON] Profile JSON structure: {:#?}", json);

        let profile = if json.is_array() {
            let profiles: Vec<Profile> = serde_json::from_value(json)?;
            profiles
                .into_iter()
                .next()
                .ok_or(NightscoutError::NoEntries)?
        } else {
            serde_json::from_value(json)?
        };

        Ok(profile)
    }

    /// Returns an `Entry` if available, or a `NightscoutError::NoEntries` if no entries are found.
    pub async fn get_entry(
        &self,
        base_url: &str,
        token: Option<&str>,
    ) -> Result<Entry, NightscoutError> {
        let entries = self
            .get_entries(base_url, NightscoutRequestOptions::default(), token)
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
        token: Option<&str>,
    ) -> Result<Vec<Entry>, NightscoutError> {
        let base = Self::parse_base_url(base_url)?;

        let url = if let Some(hours) = options.hours_back {
            let count = options.count.unwrap_or(2000); // Fetch up to 2000 entries for large time ranges
            let now = Utc::now();
            let hours_ago = now - Duration::hours(hours as i64);
            let start_timestamp = hours_ago.timestamp_millis() as u64;
            let end_timestamp = now.timestamp_millis() as u64;

            let mut query_params = format!(
                "api/v1/entries.json?find[date][$gte]={}&find[date][$lte]={}",
                start_timestamp, end_timestamp
            );

            query_params.push_str(&format!("&count={}", count));

            base.join(&query_params)?
        } else {
            let count = options.count.unwrap_or(2000); // Increase default count from u8::MAX (255) to 2000
            base.join(&format!("api/v1/entries.json?count={count}"))?
        };
        tracing::debug!("[API] Entries API URL: {}", url);
        let mut req = self.http_client.get(url.clone());

        let auth_method = token.map(AuthMethod::from_token);
        if let Some(auth) = auth_method {
            req = auth.apply_to_request(req);
            tracing::debug!(
                "[OK] Applied {} authentication for entries request",
                auth.description()
            );
        }

        tracing::debug!("[HTTP] Sending entries request to Nightscout...");
        let res = match req.send().await {
            Ok(response) => {
                tracing::debug!("[HTTP] Received entries response from Nightscout");
                response
            }
            Err(e) => return Err(Self::handle_connection_error(e, &url)),
        };

        let res = match res.error_for_status() {
            Ok(response) => {
                tracing::info!("[HTTP] Entries response status: {}", response.status());
                response
            }
            Err(e) => {
                tracing::error!("[ERROR] Entries request returned error status: {}", e);
                return Err(NightscoutError::Network(e));
            }
        };
        let entries: Vec<Entry> = res.json().await?;

        tracing::debug!(
            "[ENTRIES] Retrieved {} entries (cleaning disabled)",
            entries.len()
        );

        let mbg_count = entries.iter().filter(|e| {
            e.entry_type.as_deref() == Some("mbg") || (e.mbg.is_some() && e.mbg.unwrap_or(0.0) > 0.0)
        }).count();
        tracing::info!("[ENTRIES] Found {} entries with type='mbg' or mbg field", mbg_count);

        if entries.is_empty() {
            Err(NightscoutError::NoEntries)
        } else {
            Ok(entries)
        }
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
        hours: u16,
        token: Option<&str>,
    ) -> Result<Vec<Entry>, NightscoutError> {
        let options = NightscoutRequestOptions::default().hours_back(hours);
        self.get_entries(base_url, options, token).await
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

    /// Filters entries to include only those within the specified time range and removes duplicates
    ///
    /// This method combines time filtering and deduplication logic previously scattered across the codebase.
    /// It filters entries to only include those within the specified hours back from the current time
    /// in the user's timezone, and removes duplicate entries based on timestamp and SGV values.
    ///
    /// # Arguments
    /// * `entries` - A slice of Entry objects to filter
    /// * `hours` - Number of hours back from now to include entries for
    /// * `user_timezone` - The user's timezone string for time calculations
    ///
    /// # Returns
    /// * `Ok(Vec<Entry>)` - Vector of filtered and deduplicated entries
    /// * `Err(NightscoutError::NoEntries)` - If no entries remain after filtering
    pub fn filter_and_clean_entries(
        &self,
        entries: &[Entry],
        hours: u16,
        user_timezone: &str,
    ) -> Result<Vec<Entry>, NightscoutError> {
        if entries.is_empty() {
            return Err(NightscoutError::NoEntries);
        }

        let user_tz: chrono_tz::Tz = user_timezone.parse().unwrap_or(chrono_tz::UTC);
        let now = chrono::Utc::now().with_timezone(&user_tz);
        let cutoff_time = now - chrono::Duration::hours(hours as i64);

        // First filter by time range
        let time_filtered: Vec<&Entry> = entries
            .iter()
            .filter(|entry| {
                let entry_time = entry.millis_to_user_timezone(user_timezone);
                entry_time >= cutoff_time
            })
            .collect();

        if time_filtered.is_empty() {
            return Err(NightscoutError::NoEntries);
        }

        // Then remove duplicates
        let mut seen_ids = std::collections::HashSet::new();
        let mut processed_entries = Vec::new();

        for entry in time_filtered.into_iter().cloned() {
            // Skip entries with duplicate IDs
            if let Some(id) = &entry.id {
                if seen_ids.contains(id) {
                    continue;
                }
                seen_ids.insert(id.clone());
            }

            let entry_timestamp = entry.date.or(entry.mills).unwrap_or(0);
            let entry_sgv = (entry.sgv * 100.0) as i32;
            let entry_mbg = entry.mbg.map(|v| (v * 100.0) as i32);

            let is_duplicate = processed_entries.iter().any(|existing: &Entry| {
                let existing_timestamp = existing.date.or(existing.mills).unwrap_or(0);
                let existing_sgv = (existing.sgv * 100.0) as i32;
                let existing_mbg = existing.mbg.map(|v| (v * 100.0) as i32);

                let time_diff = (entry_timestamp as i64 - existing_timestamp as i64).abs();

                let same_value = if entry_mbg.is_some() && existing_mbg.is_some() {
                    entry_mbg == existing_mbg
                } else if entry_mbg.is_none() && existing_mbg.is_none() {
                    entry_sgv == existing_sgv
                } else {
                    false
                };

                time_diff <= 30000 && same_value
            });

            if !is_duplicate {
                processed_entries.push(entry);
            }
        }

        if processed_entries.is_empty() {
            Err(NightscoutError::NoEntries)
        } else {
            Ok(processed_entries)
        }
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

    pub async fn get_current_delta(
        &self,
        base_url: &str,
        token: Option<&str>,
    ) -> Result<Delta, NightscoutError> {
        //? Since clean entries could delete some entries due to the duplication glitch, it is
        //? safer to pull more than two. A check to verify that enough entries are available
        //? is also mandatory to avoid stupid errors.
        let options = NightscoutRequestOptions::default().count(10);
        let raw_entries = self.get_entries(base_url, options, token).await?;
        tracing::debug!(
            "[DATA] Retrieved {} raw entries for delta calculation",
            raw_entries.len()
        );

        // Filter out duplicates using the clean_entries method
        let entries = self.clean_entries(&raw_entries)?;
        tracing::debug!(
            "[DATA] After cleaning: {} entries remain for delta calculation",
            entries.len()
        );

        if entries.len() < 2 {
            return Err(NightscoutError::NoEntries);
        }

        let newer = &entries[0];
        let older = &entries[1];
        Ok(newer.get_delta(older))
    }

    /// Fetch treatments between specific timestamps
    pub async fn fetch_treatments_between(
        &self,
        base_url: &str,
        start_time: &str,
        end_time: &str,
        token: Option<&str>,
    ) -> Result<Vec<Treatment>, NightscoutError> {
        tracing::info!(
            "[TREATMENTS] Fetching treatments between {} and {}",
            start_time,
            end_time
        );

        let base = Self::parse_base_url(base_url)?;

        let query_params = format!(
            "api/v1/treatments.json?find[created_at][$gte]={}&find[created_at][$lte]={}",
            start_time, end_time
        );

        let url = base.join(&query_params)?;
        tracing::debug!("[TREATMENTS] Request URL: {}", url);

        let mut req = self.http_client.get(url.clone());

        let auth_method = token.map(AuthMethod::from_token);
        if let Some(auth) = auth_method {
            req = auth.apply_to_request(req);
            tracing::debug!("[TREATMENTS] Applied {} authentication", auth.description());
        }

        tracing::debug!("[TREATMENTS] Sending HTTP request...");
        let res = match req.send().await {
            Ok(response) => {
                tracing::debug!("[TREATMENTS] Received response from Nightscout");
                response
            }
            Err(e) => {
                tracing::error!("[TREATMENTS] HTTP request failed: {}", e);
                return Err(NightscoutError::Network(e));
            }
        };

        let res = match res.error_for_status() {
            Ok(response) => {
                tracing::info!("[TREATMENTS] Response status: {}", response.status());
                response
            }
            Err(e) => {
                tracing::error!("[TREATMENTS] Request returned error status: {}", e);
                return Err(NightscoutError::Network(e));
            }
        };

        let treatments: Vec<Treatment> = res.json().await?;
        tracing::info!("[TREATMENTS] Retrieved {} treatments", treatments.len());

        Ok(treatments)
    }

    pub async fn get_pebble_data(
        &self,
        base_url: &str,
        token: Option<&str>,
    ) -> Result<Option<PebbleData>, NightscoutError> {
        tracing::debug!("[API] Fetching pebble data from URL: '{}'", base_url);

        let base = Self::parse_base_url(base_url)?;

        let url = base.join("pebble")?;
        tracing::debug!("[API] Pebble API URL: {}", url);

        let mut req = self.http_client.get(url.clone());

        let auth_method = token.map(AuthMethod::from_token);
        if let Some(auth) = auth_method {
            req = auth.apply_to_request(req);
            tracing::debug!("[OK] Applied {} authentication", auth.description());
        }

        tracing::debug!("[HTTP] Sending pebble request...");
        let res = match req.send().await {
            Ok(response) => {
                tracing::debug!("[HTTP] Received pebble response");
                response
            }
            Err(e) => {
                tracing::warn!("[WARN] Pebble HTTP request failed: {}", e);
                return Ok(None);
            }
        };

        let res = match res.error_for_status() {
            Ok(response) => {
                tracing::info!("[HTTP] Pebble response status: {}", response.status());
                response
            }
            Err(e) => {
                tracing::warn!("[WARN] Pebble request returned error status: {}", e);
                return Ok(None);
            }
        };

        let response_text = match res.text().await {
            Ok(text) => text,
            Err(e) => {
                tracing::error!("[ERROR] Failed to read pebble response body: {}", e);
                return Ok(None);
            }
        };

        tracing::debug!("[PEBBLE] Raw response: {}", response_text);

        match serde_json::from_str::<PebbleResponse>(&response_text) {
            Ok(pebble_response) => {
                tracing::debug!("[PEBBLE] Successfully parsed pebble data");
                if !pebble_response.bgs.is_empty() {
                    Ok(Some(pebble_response.bgs[0].clone()))
                } else {
                    tracing::warn!("[PEBBLE] Pebble response bgs array is empty");
                    Ok(None)
                }
            }
            Err(e) => {
                tracing::error!("[ERROR] Failed to parse pebble JSON: {}", e);
                tracing::error!("[ERROR] Raw response was: {}", response_text);
                Ok(None)
            }
        }
    }
}
