use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use reqwest::{StatusCode, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::warn;

const PRICE_BATCH_MAX_SLUGS: usize = 50;
const DEFAULT_QUOTE_CURRENCY: &str = "USD";

#[derive(Clone)]
pub struct PriceIndexerClient {
    client: reqwest::Client,
    base_url: Url,
    token: String,
    timeout: Duration,
}

impl PriceIndexerClient {
    pub fn new(
        base_url: &str,
        token: &str,
        timeout_ms: u64,
    ) -> Result<Self, PriceIndexerClientInitError> {
        let base_url = Url::parse(base_url)
            .map_err(|error| PriceIndexerClientInitError::InvalidBaseUrl(error.to_string()))?;

        Ok(Self {
            client: reqwest::Client::new(),
            base_url,
            token: token.to_string(),
            timeout: Duration::from_millis(timeout_ms),
        })
    }

    pub fn base_host(&self) -> Option<&str> {
        self.base_url.host_str()
    }

    pub fn timeout_ms(&self) -> u128 {
        self.timeout.as_millis()
    }

    pub async fn latest_by_slug(
        &self,
        slug: &str,
        quote_currency: &str,
    ) -> Result<LatestAssetPrice, PriceLookupError> {
        let slug = slug.trim();

        if slug.is_empty() {
            return Err(PriceLookupError::InvalidSlug);
        }

        let quote_currency = normalize_quote_currency(quote_currency);
        let url = self.latest_price_url(slug, &quote_currency);
        let response = self
            .client
            .get(url)
            .bearer_auth(&self.token)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let status = response.status();
        let body = response.bytes().await.map_err(map_reqwest_error)?;

        if status.is_success() {
            let latest_price = serde_json::from_slice::<LatestPriceResponse>(&body)
                .map_err(|_| PriceLookupError::MalformedResponse)?;

            return Ok(LatestAssetPrice::from(latest_price));
        }

        Err(map_error_response(status, &body))
    }

    pub async fn latest_by_slugs(
        &self,
        slugs: &[String],
        quote_currency: &str,
    ) -> HashMap<String, LatestAssetPrice> {
        let normalized_slugs = normalize_slugs(slugs);
        let quote_currency = normalize_quote_currency(quote_currency);
        let mut prices = normalized_slugs
            .iter()
            .map(|slug| (slug.clone(), LatestAssetPrice::unavailable()))
            .collect::<HashMap<_, _>>();

        for chunk in normalized_slugs.chunks(PRICE_BATCH_MAX_SLUGS) {
            match self.latest_by_slug_chunk(chunk, &quote_currency).await {
                Ok(chunk_prices) => {
                    prices.extend(chunk_prices);
                }
                Err(error) => {
                    warn!(
                        ?error,
                        price_indexer_host = self.base_host(),
                        slugs = ?chunk,
                        "Batch price lookup failed for asset list"
                    );
                }
            }
        }

        prices
    }

    pub async fn latest_quotes_strict(
        &self,
        slugs: &[String],
        quote_currency: &str,
    ) -> Result<HashMap<String, StrictLatestQuote>, StrictPriceBatchError> {
        let normalized_slugs = normalize_slugs(slugs);
        if normalized_slugs.len() != slugs.len()
            || normalized_slugs.is_empty()
            || normalized_slugs.len() > PRICE_BATCH_MAX_SLUGS
        {
            return Err(StrictPriceBatchError::InvalidRequest);
        }

        let quote_currency = normalize_quote_currency(quote_currency);
        let response = self
            .client
            .post(self.latest_price_batch_url())
            .bearer_auth(&self.token)
            .timeout(self.timeout)
            .json(&LatestPriceBatchRequest {
                slugs: &normalized_slugs,
                quote_currency: &quote_currency,
            })
            .send()
            .await
            .map_err(map_strict_reqwest_error)?;

        let status = response.status();
        let body = response.bytes().await.map_err(map_strict_reqwest_error)?;

        if status != StatusCode::OK {
            if status.is_success() {
                return Err(StrictPriceBatchError::MalformedResponse);
            }
            return Err(map_strict_error_response(status, &body));
        }

        let response = serde_json::from_slice::<LatestPriceBatchResponse>(&body)
            .map_err(|_| StrictPriceBatchError::MalformedResponse)?;

        validate_strict_price_batch(response, &normalized_slugs, &quote_currency)
    }

    #[allow(dead_code)]
    pub async fn price_stats(
        &self,
        request: &PriceSignalRequest,
    ) -> Result<PriceStatsResponse, PriceSignalError> {
        let url = self.price_stats_url(request)?;
        self.get_signal(url).await
    }

    #[allow(dead_code)]
    pub async fn price_trend(
        &self,
        request: &PriceSignalRequest,
    ) -> Result<PriceTrendResponse, PriceSignalError> {
        let url = self.price_trend_url(request)?;
        self.get_signal(url).await
    }

    pub async fn price_stats_raw(
        &self,
        request: &PriceSignalRequest,
    ) -> Result<serde_json::Value, PriceSignalError> {
        let url = self.price_stats_url(request)?;
        self.get_signal_json(url).await
    }

    pub async fn price_trend_raw(
        &self,
        request: &PriceSignalRequest,
    ) -> Result<serde_json::Value, PriceSignalError> {
        let url = self.price_trend_url(request)?;
        self.get_signal_json(url).await
    }

    pub async fn price_series_raw(
        &self,
        request: &PriceSignalRequest,
    ) -> Result<serde_json::Value, PriceSignalError> {
        let url = self.price_series_url(request)?;
        self.get_signal_json(url).await
    }

    #[allow(dead_code)]
    pub async fn price_series(
        &self,
        request: &PriceSignalRequest,
    ) -> Result<PriceSeriesResponse, PriceSignalError> {
        let url = self.price_series_url(request)?;
        self.get_signal(url).await
    }

    async fn latest_by_slug_chunk(
        &self,
        slugs: &[String],
        quote_currency: &str,
    ) -> Result<HashMap<String, LatestAssetPrice>, PriceLookupError> {
        if slugs.is_empty() {
            return Ok(HashMap::new());
        }

        let url = self.latest_price_batch_url();
        let request = LatestPriceBatchRequest {
            slugs,
            quote_currency,
        };
        let response = self
            .client
            .post(url)
            .bearer_auth(&self.token)
            .timeout(self.timeout)
            .json(&request)
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let status = response.status();
        let body = response.bytes().await.map_err(map_reqwest_error)?;

        if status.is_success() {
            let latest_prices = serde_json::from_slice::<LatestPriceBatchResponse>(&body)
                .map_err(|_| PriceLookupError::MalformedResponse)?;

            return Ok(map_latest_price_batch_response(latest_prices));
        }

        Err(map_error_response(status, &body))
    }

    fn latest_price_url(&self, slug: &str, quote_currency: &str) -> Url {
        let mut url = self.base_url.clone();
        let base_path = url.path().trim_end_matches('/');
        url.set_path(&format!("{base_path}/prices/latest"));
        url.set_query(None);
        url.query_pairs_mut()
            .append_pair("slug", slug)
            .append_pair("quoteCurrency", quote_currency);
        url
    }

    fn latest_price_batch_url(&self) -> Url {
        let mut url = self.base_url.clone();
        let base_path = url.path().trim_end_matches('/');
        url.set_path(&format!("{base_path}/prices/latest/batch"));
        url.set_query(None);
        url
    }

    fn price_stats_url(&self, request: &PriceSignalRequest) -> Result<Url, PriceSignalError> {
        self.price_signal_url("/prices/stats", request)
    }

    fn price_trend_url(&self, request: &PriceSignalRequest) -> Result<Url, PriceSignalError> {
        self.price_signal_url("/prices/trend", request)
    }

    fn price_series_url(&self, request: &PriceSignalRequest) -> Result<Url, PriceSignalError> {
        self.price_signal_url("/prices/series", request)
    }

    fn price_signal_url(
        &self,
        endpoint_path: &str,
        request: &PriceSignalRequest,
    ) -> Result<Url, PriceSignalError> {
        let slug = request.slug.trim();
        let quote_currency = request.quote_currency.trim();
        let window = request.window.trim();
        let granularity = request.granularity.as_deref().map(str::trim);

        if slug.is_empty() || quote_currency.is_empty() || window.is_empty() {
            return Err(PriceSignalError::InvalidRequest);
        }

        let mut url = self.base_url.clone();
        let base_path = url.path().trim_end_matches('/');
        url.set_path(&format!("{base_path}{endpoint_path}"));
        url.set_query(None);
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("slug", slug);
            query.append_pair("quoteCurrency", quote_currency);
            query.append_pair("window", window);
            if let Some(granularity) = granularity {
                if granularity.is_empty() {
                    return Err(PriceSignalError::InvalidRequest);
                }
                query.append_pair("granularity", granularity);
            }
        }
        Ok(url)
    }

    #[allow(dead_code)]
    async fn get_signal<T>(&self, url: Url) -> Result<T, PriceSignalError>
    where
        T: DeserializeOwned,
    {
        let response = self
            .client
            .get(url)
            .bearer_auth(&self.token)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(map_signal_reqwest_error)?;

        let status = response.status();
        let body = response.bytes().await.map_err(map_signal_reqwest_error)?;

        if status.is_success() {
            return serde_json::from_slice::<T>(&body)
                .map_err(|_| PriceSignalError::MalformedResponse);
        }

        Err(map_signal_error_response(status, &body))
    }

    async fn get_signal_json(&self, url: Url) -> Result<serde_json::Value, PriceSignalError> {
        let response = self
            .client
            .get(url)
            .bearer_auth(&self.token)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(map_signal_reqwest_error)?;

        let status = response.status();
        let body = response.bytes().await.map_err(map_signal_reqwest_error)?;

        if status.is_success() {
            let value = serde_json::from_slice::<serde_json::Value>(&body)
                .map_err(|_| PriceSignalError::MalformedResponse)?;

            if value.is_object() {
                return Ok(value);
            }

            return Err(PriceSignalError::MalformedResponse);
        }

        Err(map_signal_error_response(status, &body))
    }
}

impl std::fmt::Debug for PriceIndexerClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PriceIndexerClient")
            .field("base_url", &self.base_url)
            .field("token", &"<redacted>")
            .field("timeout", &self.timeout)
            .finish()
    }
}

#[derive(Debug)]
pub enum PriceIndexerClientInitError {
    InvalidBaseUrl(String),
}

impl std::fmt::Display for PriceIndexerClientInitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidBaseUrl(error) => {
                write!(formatter, "invalid PRICE_INDEXER_URL: {error}")
            }
        }
    }
}

impl std::error::Error for PriceIndexerClientInitError {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LatestAssetPrice {
    pub status: PriceStatus,
    pub price: Option<String>,
    pub quote_currency: Option<String>,
    pub source_type: Option<String>,
    pub confidence_label: Option<String>,
    pub is_fallback: bool,
    pub is_derived: bool,
    pub recorded_at: Option<String>,
    pub warning: Option<String>,
}

impl LatestAssetPrice {
    pub fn unavailable() -> Self {
        Self {
            status: PriceStatus::Unavailable,
            price: None,
            quote_currency: None,
            source_type: None,
            confidence_label: None,
            is_fallback: false,
            is_derived: false,
            recorded_at: None,
            warning: None,
        }
    }
}

impl From<LatestPriceResponse> for LatestAssetPrice {
    fn from(response: LatestPriceResponse) -> Self {
        latest_asset_price_from_parts(LatestAssetPriceParts {
            freshness_status: response.freshness_status,
            price: response.price,
            quote_currency: response.quote_currency,
            source_type: response.source_type,
            confidence_label: Some(response.confidence_label),
            is_fallback: response.is_fallback,
            is_derived: response.is_derived,
            recorded_at: response.recorded_at,
        })
    }
}

impl From<BatchLatestPriceResponse> for LatestAssetPrice {
    fn from(response: BatchLatestPriceResponse) -> Self {
        latest_asset_price_from_parts(LatestAssetPriceParts {
            freshness_status: response.freshness_status,
            price: response.price,
            quote_currency: response.quote_currency,
            source_type: response.source_type,
            confidence_label: response.confidence_label,
            is_fallback: response.is_fallback,
            is_derived: response.is_derived,
            recorded_at: response.recorded_at,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceStatus {
    Available,
    Stale,
    Degraded,
    Unavailable,
}

impl PriceStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Stale => "stale",
            Self::Degraded => "degraded",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum PriceLookupError {
    Disabled,
    InvalidSlug,
    Unavailable {
        status: Option<u16>,
        code: Option<String>,
    },
    Unauthorized,
    Timeout,
    Transport,
    MalformedResponse,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StrictLatestQuote {
    Available {
        unit_price: String,
        quote_currency: String,
        recorded_at: String,
    },
    Unavailable,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StrictPriceBatchError {
    InvalidRequest,
    Unauthorized,
    ProviderUnavailable {
        status: Option<u16>,
        code: Option<String>,
    },
    Timeout,
    Transport,
    MalformedResponse,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceSignalRequest {
    pub slug: String,
    pub quote_currency: String,
    pub window: String,
    pub granularity: Option<String>,
}

#[derive(Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum PriceSignalError {
    InvalidRequest,
    NotFound,
    Unauthorized,
    UpstreamInternal,
    Timeout,
    Transport,
    MalformedResponse,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PriceStatsResponse {
    pub slug: String,
    pub asset_id: String,
    pub quote_currency: String,
    pub window: String,
    pub granularity: String,
    pub from: String,
    pub to: String,
    pub as_of: Option<String>,
    pub expected_bucket_count: u32,
    pub sample_count: u32,
    pub carry_forward_bucket_count: u32,
    pub missing_bucket_count: u32,
    pub coverage_ratio: String,
    pub first_price: Option<String>,
    pub last_price: Option<String>,
    pub min_price: Option<String>,
    pub max_price: Option<String>,
    pub mean_price: Option<String>,
    pub median_price: Option<String>,
    pub sample_std_dev: Option<String>,
    pub coefficient_of_variation: Option<String>,
    pub absolute_change: Option<String>,
    pub percent_change: Option<String>,
    pub min_timestamp: Option<String>,
    pub max_timestamp: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PriceTrendResponse {
    pub slug: String,
    pub asset_id: String,
    pub quote_currency: String,
    pub window: String,
    pub granularity: String,
    pub from: String,
    pub to: String,
    pub as_of: Option<String>,
    pub expected_bucket_count: u32,
    pub sample_count: u32,
    pub carry_forward_bucket_count: u32,
    pub missing_bucket_count: u32,
    pub coverage_ratio: String,
    pub first_price: Option<String>,
    pub last_price: Option<String>,
    pub percent_change: Option<String>,
    pub direction: String,
    pub slope: Option<String>,
    pub slope_unit: String,
    pub r_squared: Option<String>,
    pub confidence: String,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PriceSeriesResponse {
    pub asset_id: String,
    pub quote_currency: String,
    pub window: String,
    pub granularity: String,
    pub from: String,
    pub to: String,
    pub as_of: Option<String>,
    pub points: Vec<PriceSeriesPoint>,
    pub meta: PriceSeriesMeta,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PriceSeriesPoint {
    pub bucket_start: String,
    pub price: Option<String>,
    pub status: String,
    pub source_published_at: Option<String>,
    pub source_type: Option<String>,
    pub is_derived: Option<bool>,
    pub derivation_path: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PriceSeriesMeta {
    pub expected_bucket_count: u32,
    pub sample_count: u32,
    pub carry_forward_bucket_count: u32,
    pub missing_bucket_count: u32,
    pub latest_tick_published_at_used: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct LatestPriceResponse {
    asset_id: String,
    symbol: String,
    name: Option<String>,
    quote_currency: String,
    price: String,
    source_type: String,
    source_priority: u32,
    risk_category: String,
    confidence_score: u32,
    confidence_label: String,
    published_at: String,
    recorded_at: String,
    freshness_status: FreshnessStatus,
    is_fallback: bool,
    is_derived: bool,
    derivation_path: Option<Vec<String>>,
    staleness: Staleness,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LatestPriceBatchRequest<'a> {
    slugs: &'a [String],
    quote_currency: &'a str,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct LatestPriceBatchResponse {
    quote_currency: String,
    requested_count: usize,
    unique_count: usize,
    results: Vec<LatestPriceBatchResult>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct LatestPriceBatchResult {
    requested_slug: String,
    normalized_slug: String,
    asset_id: Option<String>,
    slug: Option<String>,
    name: Option<String>,
    status: PriceBatchResultStatus,
    freshness_status: Option<FreshnessStatus>,
    price: Option<BatchLatestPriceResponse>,
    error: Option<serde_json::Value>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
enum PriceBatchResultStatus {
    Found,
    Unavailable,
    Unknown,
    Error,
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct BatchLatestPriceResponse {
    asset_id: String,
    slug: String,
    quote_currency: String,
    price: String,
    source_type: String,
    published_at: Option<String>,
    recorded_at: String,
    freshness_status: FreshnessStatus,
    #[serde(default)]
    confidence_label: Option<String>,
    #[serde(default)]
    is_fallback: bool,
    #[serde(default)]
    is_derived: bool,
    staleness: Option<Staleness>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct Staleness {
    age_seconds: u64,
    is_stale: bool,
    warning_threshold_seconds: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
enum FreshnessStatus {
    Fresh,
    Stale,
    Degraded,
    Unavailable,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
struct PriceIndexerErrorEnvelope {
    error: PriceIndexerErrorBody,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct PriceIndexerErrorBody {
    code: String,
    message: String,
}

struct LatestAssetPriceParts {
    freshness_status: FreshnessStatus,
    price: String,
    quote_currency: String,
    source_type: String,
    confidence_label: Option<String>,
    is_fallback: bool,
    is_derived: bool,
    recorded_at: String,
}

fn latest_asset_price_from_parts(parts: LatestAssetPriceParts) -> LatestAssetPrice {
    match parts.freshness_status {
        FreshnessStatus::Fresh => LatestAssetPrice {
            status: PriceStatus::Available,
            price: Some(parts.price),
            quote_currency: Some(parts.quote_currency),
            source_type: Some(parts.source_type),
            confidence_label: parts.confidence_label,
            is_fallback: parts.is_fallback,
            is_derived: parts.is_derived,
            recorded_at: Some(parts.recorded_at),
            warning: None,
        },
        FreshnessStatus::Stale => LatestAssetPrice {
            status: PriceStatus::Stale,
            price: Some(parts.price),
            quote_currency: Some(parts.quote_currency),
            source_type: Some(parts.source_type),
            confidence_label: parts.confidence_label,
            is_fallback: parts.is_fallback,
            is_derived: parts.is_derived,
            recorded_at: Some(parts.recorded_at),
            warning: Some("Price may be stale.".to_string()),
        },
        FreshnessStatus::Degraded => LatestAssetPrice {
            status: PriceStatus::Degraded,
            price: Some(parts.price),
            quote_currency: Some(parts.quote_currency),
            source_type: Some(parts.source_type),
            confidence_label: parts.confidence_label,
            is_fallback: parts.is_fallback,
            is_derived: parts.is_derived,
            recorded_at: Some(parts.recorded_at),
            warning: Some("Price quality is degraded.".to_string()),
        },
        FreshnessStatus::Unavailable | FreshnessStatus::Unknown => LatestAssetPrice::unavailable(),
    }
}

fn normalize_slugs(slugs: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut normalized_slugs = Vec::new();

    for slug in slugs {
        let normalized_slug = slug.trim().to_ascii_lowercase();

        if !normalized_slug.is_empty() && seen.insert(normalized_slug.clone()) {
            normalized_slugs.push(normalized_slug);
        }
    }

    normalized_slugs
}

fn normalize_quote_currency(quote_currency: &str) -> String {
    let normalized = quote_currency.trim().to_ascii_uppercase();

    if normalized.is_empty() {
        DEFAULT_QUOTE_CURRENCY.to_string()
    } else {
        normalized
    }
}

fn map_latest_price_batch_response(
    response: LatestPriceBatchResponse,
) -> HashMap<String, LatestAssetPrice> {
    response
        .results
        .into_iter()
        .filter_map(|result| {
            let normalized_slug = result.normalized_slug.trim().to_ascii_lowercase();

            if normalized_slug.is_empty() {
                return None;
            }

            let price = match (result.status, result.price) {
                (PriceBatchResultStatus::Found, Some(price)) => LatestAssetPrice::from(price),
                _ => LatestAssetPrice::unavailable(),
            };

            Some((normalized_slug, price))
        })
        .collect()
}

fn validate_strict_price_batch(
    response: LatestPriceBatchResponse,
    requested_slugs: &[String],
    quote_currency: &str,
) -> Result<HashMap<String, StrictLatestQuote>, StrictPriceBatchError> {
    if response.quote_currency.trim().to_ascii_uppercase() != quote_currency
        || response.requested_count != requested_slugs.len()
        || response.unique_count != requested_slugs.len()
        || response.results.len() != requested_slugs.len()
    {
        return Err(StrictPriceBatchError::MalformedResponse);
    }

    let requested = requested_slugs.iter().cloned().collect::<HashSet<_>>();
    let mut quotes = HashMap::with_capacity(requested_slugs.len());

    for result in response.results {
        let requested_slug = result.requested_slug.trim().to_ascii_lowercase();
        let normalized_slug = result.normalized_slug.trim().to_ascii_lowercase();
        if requested_slug != normalized_slug
            || !requested.contains(&normalized_slug)
            || quotes.contains_key(&normalized_slug)
        {
            return Err(StrictPriceBatchError::MalformedResponse);
        }

        let quote = match result.status {
            PriceBatchResultStatus::Found => {
                let price = result
                    .price
                    .ok_or(StrictPriceBatchError::MalformedResponse)?;
                let outer_freshness = result
                    .freshness_status
                    .ok_or(StrictPriceBatchError::MalformedResponse)?;
                if result.error.is_some()
                    || result
                        .slug
                        .as_deref()
                        .is_some_and(|slug| slug.trim().to_ascii_lowercase() != normalized_slug)
                    || price.slug.trim().to_ascii_lowercase() != normalized_slug
                    || price.quote_currency.trim().to_ascii_uppercase() != quote_currency
                    || price.price.trim().is_empty()
                    || price.recorded_at.trim().is_empty()
                    || outer_freshness != price.freshness_status
                    || !matches!(
                        price.freshness_status,
                        FreshnessStatus::Fresh | FreshnessStatus::Stale | FreshnessStatus::Degraded
                    )
                {
                    return Err(StrictPriceBatchError::MalformedResponse);
                }

                StrictLatestQuote::Available {
                    unit_price: price.price,
                    quote_currency: price.quote_currency,
                    recorded_at: price.recorded_at,
                }
            }
            PriceBatchResultStatus::Unavailable => {
                if result.price.is_some()
                    || result.error.is_some()
                    || !matches!(
                        result.freshness_status,
                        None | Some(FreshnessStatus::Unavailable)
                    )
                {
                    return Err(StrictPriceBatchError::MalformedResponse);
                }
                StrictLatestQuote::Unavailable
            }
            PriceBatchResultStatus::Unknown => {
                if result.price.is_some() {
                    return Err(StrictPriceBatchError::MalformedResponse);
                }
                StrictLatestQuote::Unsupported
            }
            PriceBatchResultStatus::Error => {
                if result.price.is_some() {
                    return Err(StrictPriceBatchError::MalformedResponse);
                }
                StrictLatestQuote::Unavailable
            }
            PriceBatchResultStatus::Other => {
                return Err(StrictPriceBatchError::MalformedResponse);
            }
        };

        quotes.insert(normalized_slug, quote);
    }

    if quotes.len() != requested_slugs.len() {
        return Err(StrictPriceBatchError::MalformedResponse);
    }

    Ok(quotes)
}

fn map_reqwest_error(error: reqwest::Error) -> PriceLookupError {
    if error.is_timeout() {
        PriceLookupError::Timeout
    } else {
        PriceLookupError::Transport
    }
}

fn map_strict_reqwest_error(error: reqwest::Error) -> StrictPriceBatchError {
    if error.is_timeout() {
        StrictPriceBatchError::Timeout
    } else {
        StrictPriceBatchError::Transport
    }
}

fn map_strict_error_response(status: StatusCode, body: &[u8]) -> StrictPriceBatchError {
    let code = serde_json::from_slice::<PriceIndexerErrorEnvelope>(body)
        .ok()
        .map(|envelope| envelope.error.code);

    match status {
        StatusCode::BAD_REQUEST => StrictPriceBatchError::InvalidRequest,
        StatusCode::UNAUTHORIZED => StrictPriceBatchError::Unauthorized,
        _ => StrictPriceBatchError::ProviderUnavailable {
            status: Some(status.as_u16()),
            code,
        },
    }
}

#[allow(dead_code)]
fn map_signal_reqwest_error(error: reqwest::Error) -> PriceSignalError {
    if error.is_timeout() {
        PriceSignalError::Timeout
    } else {
        PriceSignalError::Transport
    }
}

fn map_error_response(status: StatusCode, body: &[u8]) -> PriceLookupError {
    let code = serde_json::from_slice::<PriceIndexerErrorEnvelope>(body)
        .ok()
        .map(|envelope| envelope.error.code);

    match status {
        StatusCode::BAD_REQUEST => PriceLookupError::InvalidSlug,
        StatusCode::UNAUTHORIZED => PriceLookupError::Unauthorized,
        _ => PriceLookupError::Unavailable {
            status: Some(status.as_u16()),
            code,
        },
    }
}

fn map_signal_error_response(status: StatusCode, body: &[u8]) -> PriceSignalError {
    let envelope = match serde_json::from_slice::<PriceIndexerErrorEnvelope>(body) {
        Ok(envelope) => envelope,
        Err(_) => return PriceSignalError::MalformedResponse,
    };

    match (status, envelope.error.code.as_str()) {
        (StatusCode::BAD_REQUEST, "INVALID_REQUEST") => PriceSignalError::InvalidRequest,
        (StatusCode::NOT_FOUND, "NOT_FOUND") => PriceSignalError::NotFound,
        (StatusCode::UNAUTHORIZED, "UNAUTHORIZED") => PriceSignalError::Unauthorized,
        (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR") => PriceSignalError::UpstreamInternal,
        _ => PriceSignalError::MalformedResponse,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn latest_price_json(freshness_status: &str) -> serde_json::Value {
        json!({
            "assetId": "ethereum",
            "symbol": "ETH",
            "name": "Ethereum",
            "quoteCurrency": "USD",
            "price": "3187.123456789",
            "sourceType": "coingecko",
            "sourcePriority": 10,
            "riskCategory": "normal",
            "confidenceScore": 95,
            "confidenceLabel": "high",
            "publishedAt": "2026-05-26T12:00:00Z",
            "recordedAt": "2026-05-26T12:00:05Z",
            "freshnessStatus": freshness_status,
            "isFallback": false,
            "isDerived": false,
            "derivationPath": null,
            "staleness": {
                "ageSeconds": 5,
                "isStale": false,
                "warningThresholdSeconds": 300
            }
        })
    }

    fn batch_price_json(slug: &str, freshness_status: &str) -> serde_json::Value {
        json!({
            "assetId": slug,
            "slug": slug,
            "quoteCurrency": "USD",
            "price": "2500.123456",
            "sourceType": "chainlink",
            "publishedAt": "2026-05-20T12:00:00.000Z",
            "recordedAt": "2026-05-20T12:00:01.000Z",
            "freshnessStatus": freshness_status,
            "staleness": {
                "ageSeconds": 30,
                "isStale": false,
                "warningThresholdSeconds": 300
            }
        })
    }

    fn signal_request(granularity: Option<&str>) -> PriceSignalRequest {
        PriceSignalRequest {
            slug: "ethereum".to_string(),
            quote_currency: "USD".to_string(),
            window: "24h".to_string(),
            granularity: granularity.map(str::to_string),
        }
    }

    fn assert_no_legacy_signal_params(url: &Url) {
        let query_pairs = url.query_pairs().collect::<Vec<_>>();

        for legacy_param in [
            "range",
            "resolution",
            "from",
            "to",
            "interval",
            "sourceType",
            "limit",
            "beforeId",
            "asOf",
        ] {
            assert!(
                !query_pairs.iter().any(|(key, _)| key == legacy_param),
                "unexpected legacy signal param {legacy_param}"
            );
        }
    }

    #[test]
    fn latest_price_url_identifies_asset_by_slug_and_quote_currency() {
        let client = PriceIndexerClient::new("http://price-indexer:3010/api", "secret", 2000)
            .expect("price indexer client should initialize");

        let url = client.latest_price_url("usd-coin", "MXN");
        let query_pairs = url.query_pairs().collect::<Vec<_>>();

        assert_eq!(
            url.as_str(),
            "http://price-indexer:3010/api/prices/latest?slug=usd-coin&quoteCurrency=MXN"
        );
        assert!(query_pairs
            .iter()
            .any(|(key, value)| key == "slug" && value == "usd-coin"));
        assert!(query_pairs
            .iter()
            .any(|(key, value)| key == "quoteCurrency" && value == "MXN"));
        assert!(!query_pairs.iter().any(|(key, _)| key == "symbol"));
    }

    #[test]
    fn latest_price_batch_url_uses_private_batch_endpoint() {
        let client = PriceIndexerClient::new("http://price-indexer:3010/api", "secret", 2000)
            .expect("price indexer client should initialize");

        assert_eq!(
            client.latest_price_batch_url().as_str(),
            "http://price-indexer:3010/api/prices/latest/batch"
        );
    }

    #[test]
    fn price_stats_url_uses_signal_query_model() {
        let client = PriceIndexerClient::new("http://price-indexer:3010/api", "secret", 2000)
            .expect("price indexer client should initialize");
        let url = client.price_stats_url(&signal_request(Some("1h"))).unwrap();

        assert_eq!(
            url.as_str(),
            "http://price-indexer:3010/api/prices/stats?slug=ethereum&quoteCurrency=USD&window=24h&granularity=1h"
        );
        assert_no_legacy_signal_params(&url);
    }

    #[test]
    fn price_trend_url_uses_signal_query_model() {
        let client = PriceIndexerClient::new("http://price-indexer:3010/api", "secret", 2000)
            .expect("price indexer client should initialize");
        let url = client.price_trend_url(&signal_request(Some("5m"))).unwrap();

        assert_eq!(
            url.as_str(),
            "http://price-indexer:3010/api/prices/trend?slug=ethereum&quoteCurrency=USD&window=24h&granularity=5m"
        );
        assert_no_legacy_signal_params(&url);
    }

    #[test]
    fn price_series_url_omits_granularity_when_not_provided() {
        let client = PriceIndexerClient::new("http://price-indexer:3010/api", "secret", 2000)
            .expect("price indexer client should initialize");
        let url = client.price_series_url(&signal_request(None)).unwrap();
        let query_pairs = url.query_pairs().collect::<Vec<_>>();

        assert_eq!(
            url.as_str(),
            "http://price-indexer:3010/api/prices/series?slug=ethereum&quoteCurrency=USD&window=24h"
        );
        assert!(!query_pairs.iter().any(|(key, _)| key == "granularity"));
        assert_no_legacy_signal_params(&url);
    }

    #[test]
    fn price_signal_url_uses_trimmed_query_values() {
        let client = PriceIndexerClient::new("http://price-indexer:3010/api", "secret", 2000)
            .expect("price indexer client should initialize");
        let request = PriceSignalRequest {
            slug: " ethereum ".to_string(),
            quote_currency: " USD ".to_string(),
            window: " 24h ".to_string(),
            granularity: Some(" 1h ".to_string()),
        };

        let url = client.price_stats_url(&request).unwrap();

        assert_eq!(
            url.as_str(),
            "http://price-indexer:3010/api/prices/stats?slug=ethereum&quoteCurrency=USD&window=24h&granularity=1h"
        );
    }

    #[test]
    fn price_signal_url_rejects_missing_required_values() {
        let client = PriceIndexerClient::new("http://price-indexer:3010/api", "secret", 2000)
            .expect("price indexer client should initialize");

        for request in [
            PriceSignalRequest {
                slug: " ".to_string(),
                quote_currency: "USD".to_string(),
                window: "24h".to_string(),
                granularity: None,
            },
            PriceSignalRequest {
                slug: "ethereum".to_string(),
                quote_currency: " ".to_string(),
                window: "24h".to_string(),
                granularity: None,
            },
            PriceSignalRequest {
                slug: "ethereum".to_string(),
                quote_currency: "USD".to_string(),
                window: " ".to_string(),
                granularity: None,
            },
            PriceSignalRequest {
                slug: "ethereum".to_string(),
                quote_currency: "USD".to_string(),
                window: "24h".to_string(),
                granularity: Some(" ".to_string()),
            },
        ] {
            assert_eq!(
                client.price_stats_url(&request),
                Err(PriceSignalError::InvalidRequest)
            );
        }
    }

    #[test]
    fn normalizes_and_deduplicates_batch_slugs() {
        let slugs = vec![
            " Ethereum ".to_string(),
            "ethereum".to_string(),
            "BITCOIN".to_string(),
            " ".to_string(),
            "Usdc".to_string(),
        ];

        assert_eq!(
            normalize_slugs(&slugs),
            vec![
                "ethereum".to_string(),
                "bitcoin".to_string(),
                "usdc".to_string()
            ]
        );
        assert_eq!(normalize_quote_currency(" usd "), "USD");
        assert_eq!(normalize_quote_currency(" "), "USD");
    }

    #[test]
    fn normalized_slugs_chunk_at_50_for_batch_requests() {
        let slugs = (0..51)
            .map(|index| format!("asset-{index}"))
            .collect::<Vec<_>>();
        let chunk_sizes = normalize_slugs(&slugs)
            .chunks(PRICE_BATCH_MAX_SLUGS)
            .map(<[_]>::len)
            .collect::<Vec<_>>();

        assert_eq!(chunk_sizes, vec![50, 1]);
    }

    #[test]
    fn maps_batch_results_by_normalized_slug() {
        let response = serde_json::from_value::<LatestPriceBatchResponse>(json!({
            "quoteCurrency": "USD",
            "requestedCount": 2,
            "uniqueCount": 2,
            "results": [
                {
                    "requestedSlug": "ethereum",
                    "normalizedSlug": "ethereum",
                    "assetId": "ethereum",
                    "slug": "ethereum",
                    "name": "Ethereum",
                    "status": "found",
                    "freshnessStatus": "fresh",
                    "price": batch_price_json("ethereum", "fresh"),
                    "error": null
                },
                {
                    "requestedSlug": "bitcoin",
                    "normalizedSlug": "bitcoin",
                    "assetId": "bitcoin",
                    "slug": "bitcoin",
                    "name": "Bitcoin",
                    "status": "found",
                    "freshnessStatus": "stale",
                    "price": batch_price_json("bitcoin", "stale"),
                    "error": null
                }
            ]
        }))
        .unwrap();

        let prices = map_latest_price_batch_response(response);

        assert_eq!(prices["ethereum"].status, PriceStatus::Available);
        assert_eq!(prices["ethereum"].price.as_deref(), Some("2500.123456"));
        assert_eq!(prices["ethereum"].source_type.as_deref(), Some("chainlink"));
        assert_eq!(prices["bitcoin"].status, PriceStatus::Stale);
        assert_eq!(
            prices["bitcoin"].warning.as_deref(),
            Some("Price may be stale.")
        );
    }

    #[test]
    fn maps_non_found_batch_statuses_to_unavailable() {
        let response = serde_json::from_value::<LatestPriceBatchResponse>(json!({
            "quoteCurrency": "USD",
            "requestedCount": 4,
            "uniqueCount": 4,
            "results": [
                {
                    "requestedSlug": "known-no-price",
                    "normalizedSlug": "known-no-price",
                    "assetId": "known-no-price",
                    "slug": "known-no-price",
                    "name": "Known No Price",
                    "status": "unavailable",
                    "freshnessStatus": "unavailable",
                    "price": null,
                    "error": null
                },
                {
                    "requestedSlug": "missing",
                    "normalizedSlug": "missing",
                    "assetId": null,
                    "slug": null,
                    "name": null,
                    "status": "unknown",
                    "freshnessStatus": null,
                    "price": null,
                    "error": null
                },
                {
                    "requestedSlug": "errored",
                    "normalizedSlug": "errored",
                    "assetId": null,
                    "slug": null,
                    "name": null,
                    "status": "error",
                    "freshnessStatus": null,
                    "price": null,
                    "error": {"code": "LOOKUP_FAILED"}
                },
                {
                    "requestedSlug": "bad-found",
                    "normalizedSlug": "bad-found",
                    "assetId": "bad-found",
                    "slug": "bad-found",
                    "name": "Bad Found",
                    "status": "found",
                    "freshnessStatus": "fresh",
                    "price": null,
                    "error": null
                }
            ]
        }))
        .unwrap();

        let prices = map_latest_price_batch_response(response);

        for slug in ["known-no-price", "missing", "errored", "bad-found"] {
            assert_eq!(prices[slug], LatestAssetPrice::unavailable());
        }
    }

    #[test]
    fn strict_batch_preserves_available_unavailable_and_unsupported_outcomes() {
        let response = serde_json::from_value::<LatestPriceBatchResponse>(json!({
            "quoteCurrency": "USD",
            "requestedCount": 5,
            "uniqueCount": 5,
            "results": [
                {
                    "requestedSlug": "fresh",
                    "normalizedSlug": "fresh",
                    "status": "found",
                    "freshnessStatus": "fresh",
                    "price": batch_price_json("fresh", "fresh"),
                    "error": null
                },
                {
                    "requestedSlug": "stale",
                    "normalizedSlug": "stale",
                    "status": "found",
                    "freshnessStatus": "stale",
                    "price": batch_price_json("stale", "stale"),
                    "error": null
                },
                {
                    "requestedSlug": "degraded",
                    "normalizedSlug": "degraded",
                    "status": "found",
                    "freshnessStatus": "degraded",
                    "price": batch_price_json("degraded", "degraded"),
                    "error": null
                },
                {
                    "requestedSlug": "unavailable",
                    "normalizedSlug": "unavailable",
                    "status": "unavailable",
                    "freshnessStatus": "unavailable",
                    "price": null,
                    "error": null
                },
                {
                    "requestedSlug": "unsupported",
                    "normalizedSlug": "unsupported",
                    "status": "unknown",
                    "freshnessStatus": null,
                    "price": null,
                    "error": null
                }
            ]
        }))
        .unwrap();
        let slugs = ["fresh", "stale", "degraded", "unavailable", "unsupported"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();

        let quotes = validate_strict_price_batch(response, &slugs, "USD").unwrap();

        for slug in ["fresh", "stale", "degraded"] {
            assert!(matches!(
                &quotes[slug],
                StrictLatestQuote::Available {
                    unit_price,
                    quote_currency,
                    ..
                } if unit_price == "2500.123456" && quote_currency == "USD"
            ));
        }
        assert_eq!(quotes["unavailable"], StrictLatestQuote::Unavailable);
        assert_eq!(quotes["unsupported"], StrictLatestQuote::Unsupported);
    }

    #[test]
    fn strict_batch_maps_item_errors_to_unavailable() {
        let response = serde_json::from_value::<LatestPriceBatchResponse>(json!({
            "quoteCurrency": "USD",
            "requestedCount": 1,
            "uniqueCount": 1,
            "results": [{
                "requestedSlug": "errored",
                "normalizedSlug": "errored",
                "status": "error",
                "freshnessStatus": null,
                "price": null,
                "error": {"code": "LOOKUP_FAILED"}
            }]
        }))
        .unwrap();

        let quotes =
            validate_strict_price_batch(response, &["errored".to_string()], "USD").unwrap();

        assert_eq!(quotes["errored"], StrictLatestQuote::Unavailable);
    }

    #[test]
    fn strict_batch_rejects_inconsistent_envelopes() {
        let valid = || {
            serde_json::from_value::<LatestPriceBatchResponse>(json!({
                "quoteCurrency": "USD",
                "requestedCount": 1,
                "uniqueCount": 1,
                "results": [{
                    "requestedSlug": "ethereum",
                    "normalizedSlug": "ethereum",
                    "status": "found",
                    "freshnessStatus": "fresh",
                    "price": batch_price_json("ethereum", "fresh"),
                    "error": null
                }]
            }))
            .unwrap()
        };
        let slugs = ["ethereum".to_string()];

        let mut wrong_currency = valid();
        wrong_currency.quote_currency = "MXN".to_string();
        assert_eq!(
            validate_strict_price_batch(wrong_currency, &slugs, "USD"),
            Err(StrictPriceBatchError::MalformedResponse)
        );

        let mut wrong_count = valid();
        wrong_count.requested_count = 2;
        assert_eq!(
            validate_strict_price_batch(wrong_count, &slugs, "USD"),
            Err(StrictPriceBatchError::MalformedResponse)
        );

        let mut wrong_slug = valid();
        wrong_slug.results[0].normalized_slug = "bitcoin".to_string();
        assert_eq!(
            validate_strict_price_batch(wrong_slug, &slugs, "USD"),
            Err(StrictPriceBatchError::MalformedResponse)
        );

        let mut wrong_price_currency = valid();
        wrong_price_currency.results[0]
            .price
            .as_mut()
            .unwrap()
            .quote_currency = "MXN".to_string();
        assert_eq!(
            validate_strict_price_batch(wrong_price_currency, &slugs, "USD"),
            Err(StrictPriceBatchError::MalformedResponse)
        );

        let duplicate = serde_json::from_value::<LatestPriceBatchResponse>(json!({
            "quoteCurrency": "USD",
            "requestedCount": 2,
            "uniqueCount": 2,
            "results": [
                {
                    "requestedSlug": "ethereum",
                    "normalizedSlug": "ethereum",
                    "status": "found",
                    "freshnessStatus": "fresh",
                    "price": batch_price_json("ethereum", "fresh"),
                    "error": null
                },
                {
                    "requestedSlug": "ethereum",
                    "normalizedSlug": "ethereum",
                    "status": "found",
                    "freshnessStatus": "fresh",
                    "price": batch_price_json("ethereum", "fresh"),
                    "error": null
                }
            ]
        }))
        .unwrap();
        assert_eq!(
            validate_strict_price_batch(
                duplicate,
                &["ethereum".to_string(), "bitcoin".to_string()],
                "USD"
            ),
            Err(StrictPriceBatchError::MalformedResponse)
        );
    }

    #[test]
    fn strict_batch_maps_request_wide_errors() {
        let unauthorized = serde_json::to_vec(&json!({
            "error": {"code": "UNAUTHORIZED", "message": "no"}
        }))
        .unwrap();
        assert_eq!(
            map_strict_error_response(StatusCode::UNAUTHORIZED, &unauthorized),
            StrictPriceBatchError::Unauthorized
        );

        let unavailable = serde_json::to_vec(&json!({
            "error": {"code": "INTERNAL_ERROR", "message": "no"}
        }))
        .unwrap();
        assert_eq!(
            map_strict_error_response(StatusCode::INTERNAL_SERVER_ERROR, &unavailable),
            StrictPriceBatchError::ProviderUnavailable {
                status: Some(500),
                code: Some("INTERNAL_ERROR".to_string()),
            }
        );
        assert_eq!(
            map_strict_error_response(StatusCode::BAD_REQUEST, b"not-json"),
            StrictPriceBatchError::InvalidRequest
        );
    }

    #[test]
    fn parses_success_response_without_converting_price() {
        let response =
            serde_json::from_value::<LatestPriceResponse>(latest_price_json("fresh")).unwrap();
        let price = LatestAssetPrice::from(response);

        assert_eq!(price.status, PriceStatus::Available);
        assert_eq!(price.price.as_deref(), Some("3187.123456789"));
        assert_eq!(price.quote_currency.as_deref(), Some("USD"));
        assert_eq!(price.confidence_label.as_deref(), Some("high"));
        assert_eq!(price.warning, None);
    }

    #[test]
    fn maps_freshness_statuses_to_stable_view_model() {
        for (freshness, expected_status, expected_warning) in [
            ("fresh", PriceStatus::Available, None),
            ("stale", PriceStatus::Stale, Some("Price may be stale.")),
            (
                "degraded",
                PriceStatus::Degraded,
                Some("Price quality is degraded."),
            ),
            ("unavailable", PriceStatus::Unavailable, None),
        ] {
            let response =
                serde_json::from_value::<LatestPriceResponse>(latest_price_json(freshness))
                    .unwrap();
            let price = LatestAssetPrice::from(response);

            assert_eq!(price.status, expected_status);
            assert_eq!(price.warning.as_deref(), expected_warning);
        }
    }

    #[test]
    fn parses_error_envelope_for_unavailable_responses() {
        let body = serde_json::to_vec(&json!({
            "error": {
                "code": "NOT_FOUND",
                "message": "No price found."
            }
        }))
        .unwrap();

        assert_eq!(
            map_error_response(StatusCode::NOT_FOUND, &body),
            PriceLookupError::Unavailable {
                status: Some(404),
                code: Some("NOT_FOUND".to_string())
            }
        );
    }

    #[test]
    fn maps_unauthorized_distinctly() {
        let body = serde_json::to_vec(&json!({
            "error": {
                "code": "UNAUTHORIZED",
                "message": "Unauthorized."
            }
        }))
        .unwrap();

        assert_eq!(
            map_error_response(StatusCode::UNAUTHORIZED, &body),
            PriceLookupError::Unauthorized
        );
    }

    #[test]
    fn handles_malformed_error_response_gracefully() {
        assert_eq!(
            map_error_response(StatusCode::INTERNAL_SERVER_ERROR, b"not-json"),
            PriceLookupError::Unavailable {
                status: Some(500),
                code: None
            }
        );
    }

    #[test]
    fn parses_price_stats_without_converting_decimals_or_warnings() {
        let response = serde_json::from_value::<PriceStatsResponse>(json!({
            "slug": "ethereum",
            "assetId": "00000000-0000-0000-0000-000000000001",
            "quoteCurrency": "USD",
            "window": "24h",
            "granularity": "1h",
            "from": "2026-06-01T11:00:00.000Z",
            "to": "2026-06-02T11:00:00.000Z",
            "expectedBucketCount": 24,
            "sampleCount": 1,
            "carryForwardBucketCount": 2,
            "missingBucketCount": 21,
            "coverageRatio": "0.041667",
            "firstPrice": "3812.45",
            "lastPrice": "3812.45",
            "minPrice": "3812.45",
            "maxPrice": "3812.45",
            "meanPrice": "3812.45",
            "medianPrice": "3812.45",
            "sampleStdDev": null,
            "coefficientOfVariation": null,
            "absoluteChange": "0",
            "percentChange": null,
            "minTimestamp": "2026-06-01T13:00:00.000Z",
            "maxTimestamp": "2026-06-01T13:00:00.000Z",
            "warnings": ["low_series_coverage", "custom_future_warning"],
            "futureInformationalField": "ignored"
        }))
        .unwrap();

        assert_eq!(response.coverage_ratio, "0.041667");
        assert_eq!(response.first_price.as_deref(), Some("3812.45"));
        assert_eq!(response.sample_std_dev, None);
        assert_eq!(
            response.warnings,
            vec![
                "low_series_coverage".to_string(),
                "custom_future_warning".to_string()
            ]
        );
    }

    #[test]
    fn parses_price_trend_without_converting_decimals_or_warnings() {
        let response = serde_json::from_value::<PriceTrendResponse>(json!({
            "slug": "ethereum",
            "assetId": "00000000-0000-0000-0000-000000000001",
            "quoteCurrency": "USD",
            "window": "24h",
            "granularity": "1h",
            "from": "2026-06-01T11:00:00.000Z",
            "to": "2026-06-02T11:00:00.000Z",
            "expectedBucketCount": 24,
            "sampleCount": 20,
            "carryForwardBucketCount": 2,
            "missingBucketCount": 2,
            "coverageRatio": "0.833333",
            "firstPrice": "3812.45",
            "lastPrice": "3890.10",
            "percentChange": "0.020367",
            "direction": "up",
            "slope": "0.000812",
            "slopeUnit": "per_hour",
            "rSquared": "0.640000",
            "confidence": "medium",
            "warnings": ["low_series_coverage", "missing_buckets_detected"],
            "futureInformationalField": {"ignored": true}
        }))
        .unwrap();

        assert_eq!(response.percent_change.as_deref(), Some("0.020367"));
        assert_eq!(response.slope.as_deref(), Some("0.000812"));
        assert_eq!(response.r_squared.as_deref(), Some("0.640000"));
        assert_eq!(
            response.warnings,
            vec![
                "low_series_coverage".to_string(),
                "missing_buckets_detected".to_string()
            ]
        );
    }

    #[test]
    fn parses_price_series_points_and_meta() {
        let response = serde_json::from_value::<PriceSeriesResponse>(json!({
            "assetId": "00000000-0000-0000-0000-000000000001",
            "quoteCurrency": "BTC",
            "window": "24h",
            "granularity": "1h",
            "from": "2026-06-01T11:00:00.000Z",
            "to": "2026-06-02T11:00:00.000Z",
            "points": [
                {
                    "bucketStart": "2026-06-01T11:00:00.000Z",
                    "price": "0.025",
                    "status": "observed",
                    "sourcePublishedAt": "2026-06-01T11:39:00.000Z",
                    "sourceType": "fx-derived",
                    "isDerived": true,
                    "derivationPath": ["ETH/USD", "BTC/USD"],
                    "futureInformationalField": "ignored"
                },
                {
                    "bucketStart": "2026-06-01T12:00:00.000Z",
                    "price": null,
                    "status": "missing",
                    "sourcePublishedAt": null,
                    "sourceType": null
                }
            ],
            "meta": {
                "expectedBucketCount": 24,
                "sampleCount": 1,
                "carryForwardBucketCount": 0,
                "missingBucketCount": 23,
                "latestTickPublishedAtUsed": "2026-06-01T11:39:00.000Z",
                "futureInformationalField": "ignored"
            },
            "futureInformationalField": "ignored"
        }))
        .unwrap();

        assert_eq!(response.points[0].price.as_deref(), Some("0.025"));
        assert_eq!(response.points[0].is_derived, Some(true));
        assert_eq!(
            response.points[0].derivation_path,
            Some(vec!["ETH/USD".to_string(), "BTC/USD".to_string()])
        );
        assert_eq!(response.points[1].price, None);
        assert_eq!(response.points[1].is_derived, None);
        assert_eq!(
            response.meta.latest_tick_published_at_used.as_deref(),
            Some("2026-06-01T11:39:00.000Z")
        );
    }

    #[test]
    fn maps_signal_error_envelopes_by_status_and_code() {
        for (status, code, expected) in [
            (
                StatusCode::BAD_REQUEST,
                "INVALID_REQUEST",
                PriceSignalError::InvalidRequest,
            ),
            (
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                PriceSignalError::NotFound,
            ),
            (
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                PriceSignalError::Unauthorized,
            ),
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                PriceSignalError::UpstreamInternal,
            ),
        ] {
            let body = serde_json::to_vec(&json!({
                "error": {
                    "code": code,
                    "message": "Upstream-owned message."
                }
            }))
            .unwrap();

            assert_eq!(map_signal_error_response(status, &body), expected);
        }
    }

    #[test]
    fn maps_malformed_signal_error_body_to_malformed_response() {
        assert_eq!(
            map_signal_error_response(StatusCode::INTERNAL_SERVER_ERROR, b"not-json"),
            PriceSignalError::MalformedResponse
        );
        assert_eq!(
            map_signal_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                br#"{"error":{"code":"NOT_FOUND","message":"wrong status"}}"#
            ),
            PriceSignalError::MalformedResponse
        );
    }

    #[test]
    fn malformed_signal_success_body_is_not_accepted() {
        assert!(serde_json::from_slice::<PriceStatsResponse>(b"not-json").is_err());
    }

    #[tokio::test]
    async fn price_signal_request_maps_transport_failure() {
        let Ok(listener) = std::net::TcpListener::bind("127.0.0.1:0") else {
            return;
        };
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let client = PriceIndexerClient::new(&base_url, "secret", 2000)
            .expect("price indexer client should initialize");

        let error = client
            .price_stats(&signal_request(None))
            .await
            .expect_err("closed listener should cause transport failure");

        assert_eq!(error, PriceSignalError::Transport);
    }

    #[tokio::test]
    async fn price_signal_request_maps_timeout() {
        let Ok(listener) = std::net::TcpListener::bind("127.0.0.1:0") else {
            return;
        };
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (_stream, _) = listener.accept().expect("test request should connect");
            std::thread::sleep(std::time::Duration::from_millis(100));
        });
        let client = PriceIndexerClient::new(&base_url, "secret", 10)
            .expect("price indexer client should initialize");

        let error = client
            .price_stats(&signal_request(None))
            .await
            .expect_err("held connection should time out");

        assert_eq!(error, PriceSignalError::Timeout);
        handle.join().expect("test listener thread should finish");
    }
}
