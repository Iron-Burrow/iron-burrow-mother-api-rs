use axum::{body::Bytes, extract::State, http::HeaderMap};
use tracing::warn;

use crate::adapters::http::dto::{
    erc20_transfers::Erc20TransferSearchRequest,
    filters::{
        onchain_window::OnchainWindowDTO, token_filters::TokenFilterDTO,
        transfer_direction::TransferDirectionDTO,
    },
};
use crate::adapters::http::json_body::parse_json_object_body;
use crate::adapters::http::validation::ensure_json_content_type;
use crate::application::erc20_transfers::service::{
    build_command, extraction_unavailable_placeholder, Erc20TransferCatalogResolutionIssue,
    Erc20TransferSearchError, Erc20TransferSearchInput, Erc20TransferSearchTokenFilters,
};
use crate::application::filters::{
    onchain_window::{BlockWindow, LookbackWindow, OnchainWindow, TimestampWindow},
    transfer_direction::TransferDirection,
};
use crate::domain::balance_catalog::{CatalogIntegrityIssue, CatalogResolverError};
use crate::{adapters::http::error::ApiError, state::AppState};

pub async fn search_erc20_transfers(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(), ApiError> {
    ensure_json_content_type(&headers)?;
    let request = parse_json_object_body(&body)?;
    let request = Erc20TransferSearchRequest::try_from(&request)?;
    let input = erc20_transfer_search_input_from_request(request)?;
    let command = build_command(
        input,
        state.asset_repository.clone(),
        state.config.erc20_transfers_max_token_filters,
    )
    .await
    .map_err(erc20_transfer_search_error_to_api_error)?;

    extraction_unavailable_placeholder(command)
        .await
        .map_err(erc20_transfer_search_error_to_api_error)
}

pub(crate) fn erc20_transfer_search_input_from_request(
    request: Erc20TransferSearchRequest,
) -> Result<Erc20TransferSearchInput, ApiError> {
    Ok(Erc20TransferSearchInput {
        network_slug: request.network_slug,
        address: request.address,
        direction: transfer_direction_from_dto(request.direction),
        tokens: transfer_search_token_filters_from_dto(request.tokens.unwrap_or_default()),
        window: onchain_window_from_dto(request.window)?,
    })
}

fn transfer_search_token_filters_from_dto(
    tokens: TokenFilterDTO,
) -> Erc20TransferSearchTokenFilters {
    Erc20TransferSearchTokenFilters {
        asset_slugs: tokens.asset_slugs,
        contract_addresses: tokens.contract_addresses,
    }
}

fn transfer_direction_from_dto(direction: TransferDirectionDTO) -> TransferDirection {
    match direction {
        TransferDirectionDTO::Any => TransferDirection::Any,
        TransferDirectionDTO::From => TransferDirection::From,
        TransferDirectionDTO::To => TransferDirection::To,
    }
}

fn onchain_window_from_dto(window: OnchainWindowDTO) -> Result<OnchainWindow, ApiError> {
    match window {
        OnchainWindowDTO::Block(window) => Ok(OnchainWindow::Block(BlockWindow::new(
            window.from_block,
            window.to_block,
        )?)),
        OnchainWindowDTO::Timestamp(window) => Ok(OnchainWindow::Timestamp(TimestampWindow::new(
            window.from_timestamp,
            window.to_timestamp,
        )?)),
        OnchainWindowDTO::Lookback(window) => Ok(OnchainWindow::Lookback(LookbackWindow::latest(
            window.lookback_seconds,
        )?)),
    }
}

fn erc20_transfer_search_error_to_api_error(error: Erc20TransferSearchError) -> ApiError {
    match error {
        Erc20TransferSearchError::AssetContractMappingUnavailable => {
            ApiError::asset_contract_mapping_unavailable()
        }
        Erc20TransferSearchError::Catalog(CatalogResolverError::Repository(error)) => {
            warn!(%error, "ERC-20 transfer asset catalog lookup failed");
            ApiError::asset_contract_mapping_unavailable()
        }
        Erc20TransferSearchError::Catalog(CatalogResolverError::InvalidCatalog {
            issue, ..
        }) if matches!(
            issue,
            CatalogIntegrityIssue::IncompleteMapping
                | CatalogIntegrityIssue::InvalidDecimals
                | CatalogIntegrityIssue::MalformedErc20Address
        ) =>
        {
            warn!(
                ?issue,
                "ERC-20 transfer asset catalog mapping is incomplete"
            );
            ApiError::asset_contract_mapping_unavailable()
        }
        Erc20TransferSearchError::Catalog(CatalogResolverError::InvalidCatalog {
            issue, ..
        }) => {
            warn!(
                ?issue,
                "ERC-20 transfer asset catalog is internally inconsistent"
            );
            ApiError::internal_error()
        }
        Erc20TransferSearchError::AssetNotFound => ApiError::asset_not_found(),
        Erc20TransferSearchError::AssetNotAvailableOnNetwork => {
            ApiError::asset_not_available_on_network()
        }
        Erc20TransferSearchError::AssetNotErc20OnNetwork => ApiError::asset_not_erc20_on_network(),
        Erc20TransferSearchError::TooManyTokenFilters => ApiError::too_many_token_filters(),
        Erc20TransferSearchError::InvalidCatalogResolution(issue) => {
            warn!(
                ?issue,
                message = invalid_catalog_resolution_message(issue),
                "ERC-20 transfer asset catalog resolution is invalid"
            );
            ApiError::internal_error()
        }
        Erc20TransferSearchError::ExtractionUnavailable => ApiError::extraction_unavailable(),
    }
}

fn invalid_catalog_resolution_message(issue: Erc20TransferCatalogResolutionIssue) -> &'static str {
    match issue {
        Erc20TransferCatalogResolutionIssue::ResolutionCountMismatch => {
            "catalog resolver returned an unexpected resolution count"
        }
        Erc20TransferCatalogResolutionIssue::ResolutionTargetMismatch => {
            "catalog resolver returned a mismatched resolution"
        }
    }
}

#[cfg(test)]
mod tests;
