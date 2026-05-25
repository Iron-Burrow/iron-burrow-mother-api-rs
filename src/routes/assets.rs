use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;

use crate::{
    assets::service::{AssetsResponse, AssetsService, AssetsServiceError},
    error::ApiError,
    state::AppState,
};

#[derive(Deserialize)]
pub struct AssetsQuery {
    limit: Option<String>,
}

pub async fn assets(
    State(state): State<AppState>,
    Query(params): Query<AssetsQuery>,
) -> Result<Json<AssetsResponse>, ApiError> {
    let repository = state
        .asset_repository
        .clone()
        .ok_or_else(ApiError::database_unavailable)?;
    let service = AssetsService::new(repository);
    let response = service
        .list_assets(params.limit.as_deref())
        .await
        .map_err(assets_error_to_api_error)?;

    Ok(Json(response))
}

fn assets_error_to_api_error(error: AssetsServiceError) -> ApiError {
    match error {
        AssetsServiceError::InvalidLimit => ApiError::invalid_limit(),
        AssetsServiceError::Repository(error) => {
            let _ = error;
            ApiError::database_unavailable()
        }
    }
}
