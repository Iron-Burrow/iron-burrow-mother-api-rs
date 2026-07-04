use std::collections::HashMap;

use axum::{body::Bytes, extract::State, http::HeaderMap, Json};
use tracing::warn;

use crate::adapters::http::dto::{
    accounts::OnchainAccountResponse,
    erc20_transfers::{
        requests::Erc20TransferSearchRequest, response::Erc20TransferAmount,
        response::Erc20TransferRow, response::Erc20TransferSearchLimits,
        response::Erc20TransferSearchResponse, response::Erc20TransferToken,
    },
    filters::{
        token_filters::{
            ResolvedTokenFilterDTO, TokenFilterDTO, TokenFilterResolutionDTO, TokenFilterSourceDTO,
        },
        transfer_direction::TransferDirectionDTO,
    },
    onchain_time::onchain_window::{
        BlockWindowDTO, LookbackTargetDTO, LookbackWindowDTO, OnchainWindowDTO, TimestampWindowDTO,
    },
};
use crate::adapters::http::json_body::parse_json_object_body;
use crate::adapters::http::validation::ensure_json_content_type;
use crate::application::balances::decimal::format_amount;
use crate::application::erc20_transfers::service::{
    build_search_plan, execute_search_plan, Erc20TransferCatalogResolutionIssue,
    Erc20TransferExtractionRow, Erc20TransferSearchError, Erc20TransferSearchInput,
    Erc20TransferSearchResult, Erc20TransferSearchTokenFilters, Erc20TransferTokenCatalogMetadata,
    Erc20TransferTokenFilterSource, ResolvedErc20TransferTokenFilter,
};
use crate::domain::assets::balance_catalog::{CatalogIntegrityIssue, CatalogResolverError};
use crate::domain::onchain_time::onchain_window::{
    BlockWindow, LookbackTarget, LookbackWindow, OnchainWindow, TimestampWindow,
};
use crate::domain::transfers::transfer_direction::TransferDirection;
use crate::{adapters::http::error::ApiError, state::AppState};

const ERC20_TRANSFER_SEARCH_MAX_ROWS: u64 = 5_000;

pub async fn search_erc20_transfers(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Erc20TransferSearchResponse>, ApiError> {
    ensure_json_content_type(&headers)?;
    let request = parse_json_object_body(&body)?;
    let request = Erc20TransferSearchRequest::try_from(&request)?;
    let client_ref = request.account.client_ref.clone();
    let input = erc20_transfer_search_input_from_request(request)?;
    let plan = build_search_plan(
        input,
        state.asset_repository.clone(),
        state.config.erc20_transfers_max_token_filters,
    )
    .await
    .map_err(erc20_transfer_search_error_to_api_error)?;

    let Some(bigwig_client) = state.bigwig_client.as_ref() else {
        return Err(ApiError::extraction_unavailable());
    };

    let result = execute_search_plan(plan, state.asset_repository.clone(), bigwig_client)
        .await
        .map_err(erc20_transfer_search_error_to_api_error)?;

    Ok(Json(erc20_transfer_search_response_from_result(
        result, client_ref,
    )))
}

pub(crate) fn erc20_transfer_search_input_from_request(
    request: Erc20TransferSearchRequest,
) -> Result<Erc20TransferSearchInput, ApiError> {
    let tokens = transfer_search_token_filters_from_dto(request.tokens.unwrap_or_default());

    Ok(Erc20TransferSearchInput {
        network_slug: request.account.network_slug,
        address: request.account.address,
        direction: transfer_direction_from_dto(request.direction),
        window: onchain_window_from_dto(request.window)?,
        asset_slugs: tokens.asset_slugs,
        contract_addresses: tokens.contract_addresses,
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

fn erc20_transfer_search_response_from_result(
    result: Erc20TransferSearchResult,
    client_ref: Option<String>,
) -> Erc20TransferSearchResponse {
    let Erc20TransferSearchResult {
        plan,
        extraction,
        token_metadata,
    } = result;
    let request = plan.extraction_request;
    let token_metadata = token_metadata
        .into_iter()
        .map(|metadata| (metadata.contract_address.clone(), metadata))
        .collect::<HashMap<_, _>>();

    Erc20TransferSearchResponse {
        ok: true,
        response_type: "erc20_transfer_search".to_string(),
        account: OnchainAccountResponse {
            network_slug: request.network_slug,
            address: request.address.clone(),
            client_ref,
        },
        direction: transfer_direction_to_dto(request.direction),
        window: onchain_window_to_dto(&request.window),
        token_filters: TokenFilterResolutionDTO {
            requested: TokenFilterDTO {
                asset_slugs: plan.requested_token_filters.asset_slugs,
                contract_addresses: plan.requested_token_filters.contract_addresses,
            },
            resolved_contract_addresses: plan
                .resolved_token_filters
                .into_iter()
                .map(resolved_token_filter_to_dto)
                .collect(),
        },
        transfers: extraction
            .rows
            .into_iter()
            .map(|row| erc20_transfer_row_to_dto(row, &request.address, &token_metadata))
            .collect(),
        limits: Erc20TransferSearchLimits {
            max_rows: ERC20_TRANSFER_SEARCH_MAX_ROWS,
            truncated: extraction.truncated,
        },
    }
}

fn resolved_token_filter_to_dto(
    token_filter: ResolvedErc20TransferTokenFilter,
) -> ResolvedTokenFilterDTO {
    ResolvedTokenFilterDTO {
        contract_address: token_filter.contract_address,
        asset_slug: token_filter.asset_slug,
        symbol: token_filter.symbol,
        decimals: token_filter.decimals,
        source: match token_filter.source {
            Erc20TransferTokenFilterSource::AssetSlug => TokenFilterSourceDTO::AssetSlug,
            Erc20TransferTokenFilterSource::ContractAddress => {
                TokenFilterSourceDTO::ContractAddress
            }
        },
    }
}

fn erc20_transfer_row_to_dto(
    row: Erc20TransferExtractionRow,
    watched_address: &str,
    token_metadata: &HashMap<String, Erc20TransferTokenCatalogMetadata>,
) -> Erc20TransferRow {
    let direction = transfer_row_direction(&row, watched_address);
    let contract_address = row.token.to_ascii_lowercase();
    let metadata = token_metadata.get(&contract_address);

    Erc20TransferRow {
        block_number: row.block_number,
        tx_hash: row.tx_hash,
        log_index: row.log_index,
        token: Erc20TransferToken {
            contract_address,
            asset_slug: metadata.map(|metadata| metadata.asset_slug.clone()),
            symbol: metadata.map(|metadata| metadata.symbol.clone()),
            decimals: metadata.map(|metadata| metadata.decimals),
        },
        from: row.from.clone(),
        to: row.to.clone(),
        amount: Erc20TransferAmount {
            decimal: metadata.and_then(|metadata| decimal_amount(&row.value, metadata.decimals)),
            raw: row.value,
        },
        direction,
    }
}

fn decimal_amount(raw_amount: &str, decimals: u8) -> Option<String> {
    format_amount(raw_amount, decimals)
        .ok()
        .map(trim_trailing_fractional_zeros)
}

fn trim_trailing_fractional_zeros(amount: String) -> String {
    let Some((integer, fraction)) = amount.split_once('.') else {
        return amount;
    };
    let fraction = fraction.trim_end_matches('0');

    if fraction.is_empty() {
        integer.to_string()
    } else {
        format!("{integer}.{fraction}")
    }
}

fn transfer_row_direction(
    row: &Erc20TransferExtractionRow,
    watched_address: &str,
) -> TransferDirectionDTO {
    if row.from.eq_ignore_ascii_case(watched_address) {
        TransferDirectionDTO::From
    } else if row.to.eq_ignore_ascii_case(watched_address) {
        TransferDirectionDTO::To
    } else {
        TransferDirectionDTO::Any
    }
}

fn transfer_direction_to_dto(direction: TransferDirection) -> TransferDirectionDTO {
    match direction {
        TransferDirection::Any => TransferDirectionDTO::Any,
        TransferDirection::From => TransferDirectionDTO::From,
        TransferDirection::To => TransferDirectionDTO::To,
    }
}

fn onchain_window_to_dto(window: &OnchainWindow) -> OnchainWindowDTO {
    match window {
        OnchainWindow::Block(window) => OnchainWindowDTO::Block(BlockWindowDTO {
            from_block: window.from_block,
            to_block: window.to_block,
        }),
        OnchainWindow::Timestamp(window) => OnchainWindowDTO::Timestamp(TimestampWindowDTO {
            from_timestamp: window.from_timestamp.clone(),
            to_timestamp: window.to_timestamp.clone(),
        }),
        OnchainWindow::Lookback(window) => {
            let to = match window.to {
                LookbackTarget::Latest => LookbackTargetDTO::Latest,
            };

            OnchainWindowDTO::Lookback(LookbackWindowDTO {
                lookback_seconds: window.lookback_seconds,
                to,
            })
        }
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
        Erc20TransferSearchError::WindowTooLarge => ApiError::window_too_large(),
        Erc20TransferSearchError::InvalidWindow => ApiError::invalid_window(),
        Erc20TransferSearchError::ExtractionUnavailable => ApiError::extraction_unavailable(),
        Erc20TransferSearchError::ExtractionTimeout => ApiError::extraction_timeout(),
        Erc20TransferSearchError::UpstreamProviderError => ApiError::upstream_provider_error(),
        Erc20TransferSearchError::UpstreamProviderTimeout => ApiError::upstream_provider_timeout(),
        Erc20TransferSearchError::InternalError => ApiError::internal_error(),
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
