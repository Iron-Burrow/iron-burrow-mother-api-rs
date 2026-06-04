use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use reqwest::{StatusCode, Url};
use serde::{Deserialize, Serialize};
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

    pub async fn latest_by_slug(&self, slug: &str) -> Result<LatestAssetPrice, PriceLookupError> {
        let slug = slug.trim();

        if slug.is_empty() {
            return Err(PriceLookupError::InvalidSlug);
        }

        let url = self.latest_price_url(slug);
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

    pub async fn internal_latest_by_slug(
        &self,
        slug: &str,
        currency: &str,
    ) -> Result<InternalLatestPrice, PriceLookupError> {
        let slug = slug.trim();

        if slug.is_empty() {
            return Err(PriceLookupError::InvalidSlug);
        }

        let url = self.internal_latest_price_url(slug, currency);
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
            let latest_price = serde_json::from_slice::<InternalLatestPriceResponse>(&body)
                .map_err(|_| PriceLookupError::MalformedResponse)?;

            return InternalLatestPrice::try_from(latest_price);
        }

        Err(map_error_response(status, &body))
    }

    pub async fn internal_price_series(
        &self,
        slug: &str,
        currency: &str,
        from: &str,
        to: &str,
        granularity: &str,
    ) -> Result<PriceSeries, PriceLookupError> {
        let slug = slug.trim();

        if slug.is_empty() {
            return Err(PriceLookupError::InvalidSlug);
        }

        let url = self.internal_price_series_url(slug, currency, from, to, granularity);
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
            let series = serde_json::from_slice::<PriceSeriesResponse>(&body)
                .map_err(|_| PriceLookupError::MalformedResponse)?;

            return PriceSeries::try_from(series);
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

    fn latest_price_url(&self, slug: &str) -> Url {
        let mut url = self.base_url.clone();
        let base_path = url.path().trim_end_matches('/');
        url.set_path(&format!("{base_path}/prices/latest"));
        url.set_query(None);
        url.query_pairs_mut().append_pair("slug", slug);
        url
    }

    fn internal_latest_price_url(&self, slug: &str, currency: &str) -> Url {
        let mut url = self.base_url.clone();
        let base_path = url.path().trim_end_matches('/');
        url.set_path(&format!("{base_path}/internal/v1/prices/latest"));
        url.set_query(None);
        url.query_pairs_mut()
            .append_pair("slug", slug)
            .append_pair("currency", &normalize_quote_currency(currency));
        url
    }

    fn internal_price_series_url(
        &self,
        slug: &str,
        currency: &str,
        from: &str,
        to: &str,
        granularity: &str,
    ) -> Url {
        let mut url = self.base_url.clone();
        let base_path = url.path().trim_end_matches('/');
        url.set_path(&format!("{base_path}/internal/v1/prices/series"));
        url.set_query(None);
        url.query_pairs_mut()
            .append_pair("slug", slug)
            .append_pair("currency", &normalize_quote_currency(currency))
            .append_pair("from", from)
            .append_pair("to", to)
            .append_pair("granularity", granularity);
        url
    }

    fn latest_price_batch_url(&self) -> Url {
        let mut url = self.base_url.clone();
        let base_path = url.path().trim_end_matches('/');
        url.set_path(&format!("{base_path}/prices/latest/batch"));
        url.set_query(None);
        url
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InternalLatestPrice {
    pub currency: String,
    pub value: String,
    pub published_at: String,
    pub source: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceSeries {
    pub asset: PriceSeriesAsset,
    pub currency: String,
    pub granularity: String,
    pub range_from: Option<String>,
    pub range_to: Option<String>,
    pub points: Vec<PriceSeriesPoint>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceSeriesAsset {
    pub slug: String,
    pub symbol: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceSeriesPoint {
    pub timestamp: String,
    pub price: String,
    pub source: Option<String>,
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
struct InternalLatestPriceResponse {
    #[serde(default)]
    asset: Option<InternalPriceAssetResponse>,
    #[serde(default, alias = "assetId", alias = "asset_id")]
    slug: Option<String>,
    #[serde(default)]
    symbol: Option<String>,
    #[serde(default, alias = "quoteCurrency", alias = "quote_currency")]
    currency: Option<String>,
    #[serde(default, alias = "value")]
    price: Option<String>,
    #[serde(default, alias = "sourceType", alias = "source_type")]
    source: Option<String>,
    #[serde(default, alias = "publishedAt", alias = "published_at")]
    published_at: Option<String>,
    #[serde(default, alias = "recordedAt", alias = "recorded_at")]
    recorded_at: Option<String>,
    #[serde(default, alias = "freshnessStatus", alias = "freshness_status")]
    freshness_status: Option<FreshnessStatus>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct PriceSeriesResponse {
    #[serde(default)]
    asset: Option<InternalPriceAssetResponse>,
    #[serde(default, alias = "quoteCurrency", alias = "quote_currency")]
    currency: Option<String>,
    granularity: Option<String>,
    range: Option<PriceSeriesRangeResponse>,
    #[serde(default)]
    points: Vec<PriceSeriesPointResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct InternalPriceAssetResponse {
    #[serde(default, alias = "assetId", alias = "asset_id")]
    slug: Option<String>,
    #[serde(default)]
    symbol: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct PriceSeriesRangeResponse {
    from: Option<String>,
    to: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct PriceSeriesPointResponse {
    #[serde(default, alias = "publishedAt", alias = "published_at")]
    timestamp: Option<String>,
    #[serde(default, alias = "value")]
    price: Option<String>,
    #[serde(default, alias = "sourceType", alias = "source_type")]
    source: Option<String>,
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

impl TryFrom<InternalLatestPriceResponse> for InternalLatestPrice {
    type Error = PriceLookupError;

    fn try_from(response: InternalLatestPriceResponse) -> Result<Self, Self::Error> {
        if matches!(
            response.freshness_status,
            Some(FreshnessStatus::Unavailable | FreshnessStatus::Unknown)
        ) {
            return Err(PriceLookupError::Unavailable {
                status: None,
                code: Some("PRICE_UNAVAILABLE".to_string()),
            });
        }

        Ok(Self {
            currency: response
                .currency
                .filter(|value| !value.trim().is_empty())
                .ok_or(PriceLookupError::MalformedResponse)?,
            value: response
                .price
                .filter(|value| !value.trim().is_empty())
                .ok_or(PriceLookupError::MalformedResponse)?,
            published_at: response
                .published_at
                .or(response.recorded_at)
                .filter(|value| !value.trim().is_empty())
                .ok_or(PriceLookupError::MalformedResponse)?,
            source: response
                .source
                .filter(|value| !value.trim().is_empty())
                .ok_or(PriceLookupError::MalformedResponse)?,
        })
    }
}

impl TryFrom<PriceSeriesResponse> for PriceSeries {
    type Error = PriceLookupError;

    fn try_from(response: PriceSeriesResponse) -> Result<Self, Self::Error> {
        let asset = response.asset.unwrap_or(InternalPriceAssetResponse {
            slug: Some("unknown".to_string()),
            symbol: Some("UNKNOWN".to_string()),
        });

        Ok(Self {
            asset: PriceSeriesAsset {
                slug: asset.slug.unwrap_or_else(|| "unknown".to_string()),
                symbol: asset.symbol.unwrap_or_else(|| "UNKNOWN".to_string()),
            },
            currency: response
                .currency
                .filter(|value| !value.trim().is_empty())
                .ok_or(PriceLookupError::MalformedResponse)?,
            granularity: response
                .granularity
                .filter(|value| !value.trim().is_empty())
                .ok_or(PriceLookupError::MalformedResponse)?,
            range_from: response.range.as_ref().and_then(|range| range.from.clone()),
            range_to: response.range.as_ref().and_then(|range| range.to.clone()),
            points: response
                .points
                .into_iter()
                .map(PriceSeriesPoint::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

impl TryFrom<PriceSeriesPointResponse> for PriceSeriesPoint {
    type Error = PriceLookupError;

    fn try_from(response: PriceSeriesPointResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            timestamp: response
                .timestamp
                .filter(|value| !value.trim().is_empty())
                .ok_or(PriceLookupError::MalformedResponse)?,
            price: response
                .price
                .filter(|value| !value.trim().is_empty())
                .ok_or(PriceLookupError::MalformedResponse)?,
            source: response.source,
        })
    }
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

fn map_reqwest_error(error: reqwest::Error) -> PriceLookupError {
    if error.is_timeout() {
        PriceLookupError::Timeout
    } else {
        PriceLookupError::Transport
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

    #[test]
    fn latest_price_url_identifies_asset_by_slug() {
        let client = PriceIndexerClient::new("http://price-indexer:3010/api", "secret", 2000)
            .expect("price indexer client should initialize");

        let url = client.latest_price_url("usd-coin");
        let query_pairs = url.query_pairs().collect::<Vec<_>>();

        assert_eq!(
            url.as_str(),
            "http://price-indexer:3010/api/prices/latest?slug=usd-coin"
        );
        assert!(query_pairs
            .iter()
            .any(|(key, value)| key == "slug" && value == "usd-coin"));
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
    fn internal_price_urls_use_internal_v1_query_layer_paths() {
        let client = PriceIndexerClient::new("http://price-indexer:3010", "secret", 2000)
            .expect("price indexer client should initialize");

        assert_eq!(
            client.internal_latest_price_url("ethereum", "usd").as_str(),
            "http://price-indexer:3010/internal/v1/prices/latest?slug=ethereum&currency=USD"
        );
        assert_eq!(
            client
                .internal_price_series_url(
                    "ethereum",
                    "usd",
                    "2026-05-22T00:00:00Z",
                    "2026-05-29T00:00:00Z",
                    "1h",
                )
                .as_str(),
            "http://price-indexer:3010/internal/v1/prices/series?slug=ethereum&currency=USD&from=2026-05-22T00%3A00%3A00Z&to=2026-05-29T00%3A00%3A00Z&granularity=1h"
        );
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
    fn parses_internal_latest_response_without_converting_price() {
        let response = serde_json::from_value::<InternalLatestPriceResponse>(json!({
            "asset": {
                "slug": "ethereum",
                "symbol": "ETH"
            },
            "currency": "USD",
            "price": "3811.450000",
            "published_at": "2026-05-29T00:00:00Z",
            "source": "chainlink",
            "freshness_status": "fresh"
        }))
        .unwrap();
        let price = InternalLatestPrice::try_from(response).unwrap();

        assert_eq!(price.currency, "USD");
        assert_eq!(price.value, "3811.450000");
        assert_eq!(price.published_at, "2026-05-29T00:00:00Z");
        assert_eq!(price.source, "chainlink");
    }

    #[test]
    fn parses_internal_price_series_response() {
        let response = serde_json::from_value::<PriceSeriesResponse>(json!({
            "asset": {
                "slug": "ethereum",
                "symbol": "ETH"
            },
            "currency": "USD",
            "granularity": "1h",
            "range": {
                "from": "2026-05-22T00:00:00Z",
                "to": "2026-05-29T00:00:00Z"
            },
            "points": [
                {
                    "timestamp": "2026-05-22T00:00:00Z",
                    "price": "3720.120000",
                    "source": "chainlink"
                }
            ]
        }))
        .unwrap();
        let series = PriceSeries::try_from(response).unwrap();

        assert_eq!(series.asset.slug, "ethereum");
        assert_eq!(series.currency, "USD");
        assert_eq!(series.granularity, "1h");
        assert_eq!(series.points[0].price, "3720.120000");
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
}
