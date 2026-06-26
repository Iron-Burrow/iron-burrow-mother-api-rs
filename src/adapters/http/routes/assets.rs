use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::{
    adapters::price_indexer::{PriceIndexerClient, PriceSignalError, PriceSignalRequest},
    application::assets::service::{
        AssetEnrichmentInclude, AssetEnrichmentParams, AssetEnrichmentQuery, AssetResponse,
        AssetsResponse, AssetsService, AssetsServiceError,
    },
    error::ApiError,
    state::AppState,
};

const DEFAULT_QUOTE_CURRENCY: &str = "USD";
const DEFAULT_SIGNAL_WINDOW: &str = "24h";

#[derive(Deserialize)]
pub struct AssetsQuery {
    limit: Option<String>,
}

#[derive(Deserialize)]
pub struct AssetDetailQuery {
    include: Option<String>,
    #[serde(rename = "quoteCurrency")]
    quote_currency: Option<String>,
    window: Option<String>,
    granularity: Option<String>,
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
    Query(params): Query<AssetDetailQuery>,
) -> Result<Json<AssetResponse>, ApiError> {
    let quote_currency = parse_quote_currency(params.quote_currency.as_deref())?;
    let repository = state
        .asset_repository
        .clone()
        .ok_or_else(ApiError::database_unavailable)?;
    let service = AssetsService::new(repository, state.price_indexer_client.clone());
    let enrichment_query = parse_asset_enrichment_query(&slug, params);
    let response = service
        .get_asset(&slug, &quote_currency, enrichment_query)
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

fn parse_asset_enrichment_query(
    slug: &str,
    params: AssetDetailQuery,
) -> Option<AssetEnrichmentQuery> {
    let include = parse_asset_enrichment_include(params.include.as_deref());

    if include.is_empty() {
        return None;
    }

    let signal_params = PriceSignalQuery {
        quote_currency: params.quote_currency,
        window: params.window,
        granularity: params.granularity,
    };

    let params = parse_signal_request(slug.to_string(), signal_params)
        .map(|request| AssetEnrichmentParams {
            slug: request.slug,
            quote_currency: request.quote_currency,
            window: request.window,
            granularity: request.granularity,
        })
        .ok();

    Some(AssetEnrichmentQuery { include, params })
}

fn parse_asset_enrichment_include(raw_include: Option<&str>) -> Vec<AssetEnrichmentInclude> {
    let Some(raw_include) = raw_include else {
        return Vec::new();
    };

    let mut include = Vec::new();

    for token in raw_include.split(',') {
        let normalized = token.trim().to_ascii_lowercase();
        let parsed = match normalized.as_str() {
            "pricestats" => Some(AssetEnrichmentInclude::PriceStats),
            "pricetrend" => Some(AssetEnrichmentInclude::PriceTrend),
            "priceseries" => Some(AssetEnrichmentInclude::PriceSeries),
            _ => None,
        };

        if let Some(parsed) = parsed.filter(|parsed| !include.contains(parsed)) {
            include.push(parsed);
        }
    }

    include
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

    let quote_currency = parse_quote_currency(params.quote_currency.as_deref())?;
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

fn parse_quote_currency(raw_quote_currency: Option<&str>) -> Result<String, ApiError> {
    let quote_currency = raw_quote_currency
        .unwrap_or(DEFAULT_QUOTE_CURRENCY)
        .trim()
        .to_ascii_uppercase();

    if matches!(quote_currency.as_str(), "USD" | "MXN" | "USDC" | "BTC") {
        Ok(quote_currency)
    } else {
        Err(ApiError::invalid_request())
    }
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
