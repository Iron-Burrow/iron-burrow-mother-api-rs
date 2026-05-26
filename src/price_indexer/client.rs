use std::time::Duration;

use axum::http::StatusCode;
use reqwest::Url;
use serde::{Deserialize, Serialize};

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

    pub async fn latest_by_symbol(
        &self,
        symbol: &str,
    ) -> Result<LatestAssetPrice, PriceLookupError> {
        let symbol = symbol.trim();

        if symbol.is_empty() {
            return Err(PriceLookupError::InvalidSymbol);
        }

        let url = self.latest_price_url(symbol);
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

    fn latest_price_url(&self, symbol: &str) -> Url {
        let mut url = self.base_url.clone();
        let base_path = url.path().trim_end_matches('/');
        url.set_path(&format!("{base_path}/prices/latest"));
        url.set_query(None);
        url.query_pairs_mut().append_pair("symbol", symbol);
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
        match response.freshness_status {
            FreshnessStatus::Fresh => Self {
                status: PriceStatus::Available,
                price: Some(response.price),
                quote_currency: Some(response.quote_currency),
                source_type: Some(response.source_type),
                confidence_label: Some(response.confidence_label),
                is_fallback: response.is_fallback,
                is_derived: response.is_derived,
                recorded_at: Some(response.recorded_at),
                warning: None,
            },
            FreshnessStatus::Stale => Self {
                status: PriceStatus::Stale,
                price: Some(response.price),
                quote_currency: Some(response.quote_currency),
                source_type: Some(response.source_type),
                confidence_label: Some(response.confidence_label),
                is_fallback: response.is_fallback,
                is_derived: response.is_derived,
                recorded_at: Some(response.recorded_at),
                warning: Some("Price may be stale.".to_string()),
            },
            FreshnessStatus::Degraded => Self {
                status: PriceStatus::Degraded,
                price: Some(response.price),
                quote_currency: Some(response.quote_currency),
                source_type: Some(response.source_type),
                confidence_label: Some(response.confidence_label),
                is_fallback: response.is_fallback,
                is_derived: response.is_derived,
                recorded_at: Some(response.recorded_at),
                warning: Some("Price quality is degraded.".to_string()),
            },
            FreshnessStatus::Unavailable | FreshnessStatus::Unknown => Self::unavailable(),
        }
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
    InvalidSymbol,
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
        StatusCode::BAD_REQUEST => PriceLookupError::InvalidSymbol,
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
}
