use axum::{body::Bytes, extract::State, http::HeaderMap};

use crate::adapters::http::dto::erc20_transfers::Erc20TransferSearchRequest;
use crate::adapters::http::json_body::parse_json_object_body;
use crate::adapters::http::validation::ensure_json_content_type;
use crate::application::erc20_transfers::service::{
    build_command, extraction_unavailable_placeholder,
};
use crate::{adapters::http::error::ApiError, state::AppState};

pub async fn search_erc20_transfers(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(), ApiError> {
    ensure_json_content_type(&headers)?;
    let request = parse_json_object_body(&body)?;
    let request = Erc20TransferSearchRequest::try_from(&request)?;
    let command = build_command(
        request,
        state.asset_repository.clone(),
        state.config.erc20_transfers_max_token_filters,
    )
    .await?;

    extraction_unavailable_placeholder(command).await
}

#[cfg(test)]
mod tests;
