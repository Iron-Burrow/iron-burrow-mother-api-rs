use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

use crate::{
    assets::service::{AssetResponse, AssetsResponse, AssetsService, AssetsServiceError},
    error::ApiError,
    state::AppState,
};

#[derive(Deserialize)]
pub struct AssetsQuery {
    limit: Option<String>,
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
