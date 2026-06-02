use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::{
    assets::service::{AssetResponse, AssetsResponse, AssetsService, AssetsServiceError},
    error::ApiError,
    price_indexer::{PriceIndexerClient, PriceSignalError, PriceSignalRequest},
    state::AppState,
};

const DEFAULT_SIGNAL_QUOTE_CURRENCY: &str = "USD";
const DEFAULT_SIGNAL_WINDOW: &str = "24h";

#[derive(Deserialize)]
pub struct AssetsQuery {
    limit: Option<String>,
}

#[derive(Deserialize)]
pub struct PriceSignalQuery {
    #[serde(rename = "quoteCurrency")]
    quote_currency: Option<String>,
    window: Option<String>,
    granularity: Option<String>,
}

#[derive(Serialize)]
pub struct PriceSignalResponse {
    ok: bool,
    #[serde(rename = "type")]
    response_type: &'static str,
    signal: serde_json::Value,
}

impl PriceSignalResponse {
    fn price_stats(signal: serde_json::Value) -> Self {
        Self {
            ok: true,
            response_type: "price_stats",
            signal,
        }
    }

    fn price_trend(signal: serde_json::Value) -> Self {
        Self {
            ok: true,
            response_type: "price_trend",
            signal,
        }
    }
}

pub async fn list_assets(
    State(state): State<AppState>,
    Query(params): Query<AssetsQuery>,
) -> Result<Json<AssetsResponse>, ApiError> {
    let repository = state
        .asset_repository
        .clone()
        .ok_or_else(ApiError::database_unavailable)?;
    let service = AssetsService::new(repository, state.price_indexer_client.clone());
    let response = service
        .list_assets(params.limit.as_deref())
        .await
        .map_err(assets_error_to_api_error)?;

    Ok(Json(response))
}

pub async fn get_asset(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<AssetResponse>, ApiError> {
    let repository = state
        .asset_repository
        .clone()
        .ok_or_else(ApiError::database_unavailable)?;
    let service = AssetsService::new(repository, state.price_indexer_client.clone());
    let response = service
        .get_asset(&slug)
        .await
        .map_err(assets_error_to_api_error)?;

    Ok(Json(response))
}

pub async fn get_price_stats_signal(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(params): Query<PriceSignalQuery>,
) -> Result<Json<PriceSignalResponse>, ApiError> {
    let request = parse_signal_request(slug, params)?;
    let client = state
        .price_indexer_client
        .as_ref()
        .ok_or_else(ApiError::price_indexer_unavailable)?;
    let signal = client
        .price_stats_raw(&request)
        .await
        .map_err(|error| signal_error_to_api_error(client, &request, "price_stats", error))?;

    Ok(Json(PriceSignalResponse::price_stats(signal)))
}

pub async fn get_price_trend_signal(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(params): Query<PriceSignalQuery>,
) -> Result<Json<PriceSignalResponse>, ApiError> {
    let request = parse_signal_request(slug, params)?;
    let client = state
        .price_indexer_client
        .as_ref()
        .ok_or_else(ApiError::price_indexer_unavailable)?;
    let signal = client
        .price_trend_raw(&request)
        .await
        .map_err(|error| signal_error_to_api_error(client, &request, "price_trend", error))?;

    Ok(Json(PriceSignalResponse::price_trend(signal)))
}

fn assets_error_to_api_error(error: AssetsServiceError) -> ApiError {
    match error {
        AssetsServiceError::InvalidLimit => ApiError::invalid_limit(),
        AssetsServiceError::AssetNotFound => ApiError::asset_not_found(),
        AssetsServiceError::Repository(error) => {
            let _ = error;
            ApiError::database_unavailable()
        }
    }
}

fn parse_signal_request(
    slug: String,
    params: PriceSignalQuery,
) -> Result<PriceSignalRequest, ApiError> {
    let slug = slug.trim().to_string();

    if slug.is_empty() {
        return Err(ApiError::invalid_request());
    }

    let quote_currency = params
        .quote_currency
        .as_deref()
        .unwrap_or(DEFAULT_SIGNAL_QUOTE_CURRENCY)
        .trim()
        .to_ascii_uppercase();
    let window = params
        .window
        .as_deref()
        .unwrap_or(DEFAULT_SIGNAL_WINDOW)
        .trim()
        .to_string();
    let granularity = match params.granularity.as_deref().map(str::trim) {
        Some("") => return Err(ApiError::invalid_request()),
        Some(value) => Some(value.to_string()),
        None => None,
    };

    if !matches!(quote_currency.as_str(), "USD" | "MXN" | "USDC" | "BTC") {
        return Err(ApiError::invalid_request());
    }

    if !matches!(window.as_str(), "1h" | "24h" | "7d" | "30d") {
        return Err(ApiError::invalid_request());
    }

    if let Some(granularity) = granularity.as_deref() {
        let allowed = match window.as_str() {
            "1h" => granularity == "5m",
            "24h" => matches!(granularity, "5m" | "1h"),
            "7d" => granularity == "1h",
            "30d" => granularity == "1d",
            _ => false,
        };

        if !allowed {
            return Err(ApiError::invalid_request());
        }
    }

    Ok(PriceSignalRequest {
        slug,
        quote_currency,
        window,
        granularity,
    })
}

fn signal_error_to_api_error(
    client: &PriceIndexerClient,
    request: &PriceSignalRequest,
    signal_type: &'static str,
    error: PriceSignalError,
) -> ApiError {
    warn!(
        ?error,
        signal_type,
        asset_slug = request.slug.as_str(),
        quote_currency = request.quote_currency.as_str(),
        window = request.window.as_str(),
        granularity = request.granularity.as_deref(),
        price_indexer_host = client.base_host(),
        timeout_ms = client.timeout_ms(),
        "Price signal lookup failed"
    );

    match error {
        PriceSignalError::InvalidRequest => ApiError::invalid_request(),
        PriceSignalError::NotFound => ApiError::asset_not_found(),
        PriceSignalError::Unauthorized => ApiError::upstream_auth_failed(),
        PriceSignalError::UpstreamInternal => ApiError::price_indexer_error(),
        PriceSignalError::Timeout | PriceSignalError::Transport => {
            ApiError::price_indexer_unavailable()
        }
        PriceSignalError::MalformedResponse => ApiError::upstream_invalid_response(),
    }
}
