use axum::{
    body::Bytes,
    extract::{Extension, State},
    http::HeaderMap,
    Json,
};

use crate::adapters::http::{
    auth::{require_network_scopes, ApiKeyPrincipal},
    dto::balances::{
        requests::BulkBalanceRequest, requests::SingleBalanceRequest, BulkBalanceResponse,
        SingleBalanceResponse,
    },
    error::ApiError,
};
use crate::adapters::http::{presenters::balances::BalancesResponsePresenter, state::HttpState};
use crate::application::balances::command::GetBalancesCommand;

mod error;

use error::{balance_assembler_error_to_api_error, balance_service_error_to_api_error};

pub async fn resolve_single_balance(
    State(state): State<HttpState>,
    principal: Option<Extension<ApiKeyPrincipal>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<SingleBalanceResponse>, ApiError> {
    let request = SingleBalanceRequest::try_from((&headers, &body))?;
    let command = GetBalancesCommand::try_from(request)?;
    let network_slugs = command
        .accounts()
        .iter()
        .map(|account| account.network_slug.clone())
        .collect::<Vec<_>>();
    require_network_scopes(
        &state,
        principal.as_ref().map(|principal| &principal.0),
        crate::domain::capabilities::Capability::BalancesRead,
        &network_slugs,
    )
    .await?;
    let result = resolve_balances(&state, command).await?;

    let response = BalancesResponsePresenter
        .single(result)
        .map_err(balance_assembler_error_to_api_error)?;

    Ok(Json(response))
}

pub async fn resolve_bulk_balances(
    State(state): State<HttpState>,
    principal: Option<Extension<ApiKeyPrincipal>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<BulkBalanceResponse>, ApiError> {
    let request = BulkBalanceRequest::try_from((&headers, &body))?;
    let command = GetBalancesCommand::try_from(request)?;
    let network_slugs = command
        .accounts()
        .iter()
        .map(|account| account.network_slug.clone())
        .collect::<Vec<_>>();
    require_network_scopes(
        &state,
        principal.as_ref().map(|principal| &principal.0),
        crate::domain::capabilities::Capability::BalancesRead,
        &network_slugs,
    )
    .await?;
    let result = resolve_balances(&state, command).await?;

    let response = BalancesResponsePresenter.bulk(result);

    Ok(Json(response))
}

async fn resolve_balances(
    state: &HttpState,
    command: GetBalancesCommand,
) -> Result<crate::application::balances::result::GetBalancesResult, ApiError> {
    state
        .balance_service
        .resolve(command)
        .await
        .map_err(balance_service_error_to_api_error)
}

#[cfg(test)]
mod tests;
