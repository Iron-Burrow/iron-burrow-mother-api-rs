use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;

use crate::{
    adapters::http::error::ApiError,
    application::resolve::{
        query::{parse_query, QueryValidationError},
        service::{ResolveResponse, ResolveService},
    },
    state::AppState,
};

#[derive(Deserialize)]
pub struct ResolveQuery {
    q: Option<String>,
}

pub async fn resolve(
    State(state): State<AppState>,
    Query(params): Query<ResolveQuery>,
) -> Result<Json<ResolveResponse>, ApiError> {
    let query = parse_query(params.q.as_deref()).map_err(query_error_to_api_error)?;
    let repository = state
        .asset_repository
        .clone()
        .ok_or_else(ApiError::database_unavailable)?;
    let service = ResolveService::new(repository);
    let response = service
        .resolve(query)
        .await
        .map_err(|_| ApiError::database_unavailable())?;

    Ok(Json(response))
}

fn query_error_to_api_error(error: QueryValidationError) -> ApiError {
    match error {
        QueryValidationError::Missing => {
            ApiError::missing_query("Query parameter `q` is required.")
        }
        QueryValidationError::Empty => {
            ApiError::missing_query("Query parameter `q` must not be empty.")
        }
        QueryValidationError::TooLong => ApiError::query_too_long(),
    }
}
