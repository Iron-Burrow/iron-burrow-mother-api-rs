use axum::{
    extract::{Path, RawQuery, State},
    Json,
};

use crate::{
    error::ApiError,
    signals::service::{
        LatestPriceResponse, PriceSignalService, PriceSignalServiceError, PriceStatsResponse,
        PriceTrendResponse,
    },
    state::AppState,
};

pub async fn latest_price(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<LatestPriceResponse>, ApiError> {
    let repository = state
        .asset_repository
        .clone()
        .ok_or_else(ApiError::database_unavailable)?;
    let service = PriceSignalService::new(repository, state.price_indexer_client.clone());
    let response = service
        .latest_price(&slug)
        .await
        .map_err(signal_error_to_api_error)?;

    Ok(Json(response))
}

pub async fn price_stats(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    RawQuery(raw_query): RawQuery,
) -> Result<Json<PriceStatsResponse>, ApiError> {
    let repository = state
        .asset_repository
        .clone()
        .ok_or_else(ApiError::database_unavailable)?;
    let service = PriceSignalService::new(repository, state.price_indexer_client.clone());
    let response = service
        .price_stats(&slug, raw_query.as_deref())
        .await
        .map_err(signal_error_to_api_error)?;

    Ok(Json(response))
}

pub async fn price_trend(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    RawQuery(raw_query): RawQuery,
) -> Result<Json<PriceTrendResponse>, ApiError> {
    let repository = state
        .asset_repository
        .clone()
        .ok_or_else(ApiError::database_unavailable)?;
    let service = PriceSignalService::new(repository, state.price_indexer_client.clone());
    let response = service
        .price_trend(&slug, raw_query.as_deref())
        .await
        .map_err(signal_error_to_api_error)?;

    Ok(Json(response))
}

fn signal_error_to_api_error(error: PriceSignalServiceError) -> ApiError {
    match error {
        PriceSignalServiceError::InvalidRange(error) => {
            ApiError::invalid_price_signal_query(error.message())
        }
        PriceSignalServiceError::AssetNotFound => ApiError::asset_not_found(),
        PriceSignalServiceError::Repository(error) => {
            let _ = error;
            ApiError::database_unavailable()
        }
        PriceSignalServiceError::PriceIndexer(error) => {
            let _ = error;
            ApiError::price_indexer_unavailable()
        }
        PriceSignalServiceError::PriceIndexerUnavailable
        | PriceSignalServiceError::CalculationFailed
        | PriceSignalServiceError::MalformedPricePoint => ApiError::price_indexer_unavailable(),
    }
}
