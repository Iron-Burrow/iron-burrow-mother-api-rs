use axum::{body::Bytes, extract::State, http::HeaderMap, Json};

use crate::adapters::http::presenters::balances::BalancesResponsePresenter;
use crate::application::balances::command::GetBalancesCommand;
use crate::application::balances::result::GetBalancesResult;
use crate::{
    adapters::http::{
        dto::balances::{
            requests::BulkBalanceRequest, requests::SingleBalanceRequest, BulkBalanceResponse,
            SingleBalanceResponse,
        },
        error::ApiError,
    },
    application::balances::{
        catalog::CatalogBalanceTargetResolver, quote::PriceQuoteClient,
        service::BalanceSnapshotService,
    },
    state::AppState,
};

mod error;

use error::{balance_assembler_error_to_api_error, balance_service_error_to_api_error};

pub async fn resolve_single_balance(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<SingleBalanceResponse>, ApiError> {
    let request = SingleBalanceRequest::try_from((&headers, &body))?;
    let command = GetBalancesCommand::try_from(request)?;
    let result = resolve_balances(&state, command).await?;

    let response = BalancesResponsePresenter
        .single(result)
        .map_err(balance_assembler_error_to_api_error)?;

    Ok(Json(response))
}

pub async fn resolve_bulk_balances(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<BulkBalanceResponse>, ApiError> {
    let request = BulkBalanceRequest::try_from((&headers, &body))?;
    let command = GetBalancesCommand::try_from(request)?;
    let result = resolve_balances(&state, command).await?;

    let response = BalancesResponsePresenter.bulk(result);

    Ok(Json(response))
}

async fn resolve_balances(
    state: &AppState,
    command: GetBalancesCommand,
) -> Result<GetBalancesResult, ApiError> {
    let repository = state
        .asset_repository
        .clone()
        .ok_or_else(ApiError::asset_network_map_unavailable)?;
    let service = BalanceSnapshotService::new(
        CatalogBalanceTargetResolver::new(repository),
        state.bigwig_client.clone(),
        state
            .price_indexer_client
            .clone()
            .map(PriceQuoteClient::new),
    );

    service
        .resolve(command)
        .await
        .map_err(balance_service_error_to_api_error)
}

#[cfg(test)]
mod tests;
