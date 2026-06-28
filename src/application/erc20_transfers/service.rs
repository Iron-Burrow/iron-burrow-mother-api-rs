use std::collections::HashSet;
use tracing::warn;

use crate::adapters::http::dto::filters::token_filters::TokenFilterDTO;
use crate::adapters::http::dto::filters::transfer_direction::TransferDirectionDTO;
use crate::adapters::http::{dto::erc20_transfers::Erc20TransferSearchRequest, error::ApiError};
use crate::adapters::postgres::global_assets::GlobalAssetRepository;
use crate::application::balances::catalog::{
    BalanceTargetResolution, CatalogBalanceTargetResolver,
};
use crate::application::filters::onchain_window::OnchainWindow;
use crate::application::filters::transfer_direction::TransferDirection;
use crate::domain::balance_catalog::{
    BalanceTargetKind, CatalogIntegrityIssue, CatalogResolverError,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Erc20TransferSearchCommand {
    pub network_slug: String,
    pub address: String,
    pub direction: TransferDirection,
    pub tokens: Erc20TransferCommandTokenFilters,
    pub window: OnchainWindow,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Erc20TransferCommandTokenFilters {
    pub contract_addresses: Vec<String>,
}

pub(crate) async fn build_command(
    request: Erc20TransferSearchRequest,
    repository: Option<GlobalAssetRepository>,
    max_token_filters: u64,
) -> Result<Erc20TransferSearchCommand, ApiError> {
    let tokens = request.tokens.unwrap_or_default();
    let contract_addresses =
        resolve_token_filters(repository, &request.network_slug, tokens).await?;
    enforce_token_filter_limit(&contract_addresses, max_token_filters)?;

    Ok(Erc20TransferSearchCommand {
        network_slug: request.network_slug,
        address: request.address.to_ascii_lowercase(),
        direction: command_direction(request.direction),
        tokens: Erc20TransferCommandTokenFilters { contract_addresses },
        window: OnchainWindow::try_from(request.window)?,
    })
}

pub(crate) async fn extraction_unavailable_placeholder(
    _command: Erc20TransferSearchCommand,
) -> Result<(), ApiError> {
    Err(ApiError::extraction_unavailable())
}

fn command_direction(direction: TransferDirectionDTO) -> TransferDirection {
    match direction {
        TransferDirectionDTO::Any => TransferDirection::Any,
        TransferDirectionDTO::From => TransferDirection::From,
        TransferDirectionDTO::To => TransferDirection::To,
    }
}

async fn resolve_token_filters(
    repository: Option<GlobalAssetRepository>,
    network_slug: &str,
    tokens: TokenFilterDTO,
) -> Result<Vec<String>, ApiError> {
    let mut contract_addresses = Vec::new();
    let mut seen = HashSet::new();

    if !tokens.asset_slugs.is_empty() {
        let repository = repository.ok_or_else(ApiError::asset_contract_mapping_unavailable)?;
        let resolver = CatalogBalanceTargetResolver::new(repository);
        let resolved_contracts = resolver
            .resolve_network(network_slug, &tokens.asset_slugs)
            .await
            .map_err(catalog_resolver_error_to_api_error)
            .and_then(|resolutions| {
                resolved_contract_addresses_from_catalog(
                    network_slug,
                    &tokens.asset_slugs,
                    resolutions,
                )
            })?;

        for contract_address in resolved_contracts {
            push_unique_contract_address(&mut contract_addresses, &mut seen, contract_address);
        }
    }

    for contract_address in tokens.contract_addresses {
        push_unique_contract_address(&mut contract_addresses, &mut seen, contract_address);
    }

    Ok(contract_addresses)
}

fn resolved_contract_addresses_from_catalog(
    network_slug: &str,
    requested_asset_slugs: &[String],
    resolutions: Vec<BalanceTargetResolution>,
) -> Result<Vec<String>, ApiError> {
    if resolutions.len() != requested_asset_slugs.len() {
        warn!(
            network_slug,
            requested_count = requested_asset_slugs.len(),
            resolution_count = resolutions.len(),
            "ERC-20 transfer catalog resolver returned an unexpected resolution count"
        );
        return Err(ApiError::internal_error());
    }

    let mut contract_addresses = Vec::with_capacity(resolutions.len());

    for (requested_asset_slug, resolution) in requested_asset_slugs.iter().zip(resolutions) {
        match resolution {
            BalanceTargetResolution::Resolved(target) => {
                if target.network_slug != network_slug || target.asset_slug != *requested_asset_slug
                {
                    warn!(
                        network_slug,
                        requested_asset_slug,
                        resolved_network_slug = target.network_slug,
                        resolved_asset_slug = target.asset_slug,
                        "ERC-20 transfer catalog resolver returned a mismatched resolution"
                    );
                    return Err(ApiError::internal_error());
                }

                match target.kind {
                    BalanceTargetKind::Erc20 { contract_address } => {
                        contract_addresses.push(contract_address);
                    }
                    BalanceTargetKind::Native => {
                        return Err(ApiError::asset_not_erc20_on_network());
                    }
                }
            }
            BalanceTargetResolution::UnsupportedAsset { .. } => {
                return Err(ApiError::asset_not_found());
            }
            BalanceTargetResolution::UnsupportedNetwork { .. }
            | BalanceTargetResolution::UnsupportedPair { .. } => {
                return Err(ApiError::asset_not_available_on_network());
            }
            BalanceTargetResolution::UnsupportedTokenStandard { .. } => {
                return Err(ApiError::asset_not_erc20_on_network());
            }
        }
    }

    Ok(contract_addresses)
}

fn catalog_resolver_error_to_api_error(error: CatalogResolverError) -> ApiError {
    match error {
        CatalogResolverError::Repository(error) => {
            warn!(%error, "ERC-20 transfer asset catalog lookup failed");
            ApiError::asset_contract_mapping_unavailable()
        }
        CatalogResolverError::InvalidCatalog { issue, .. }
            if matches!(
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
        CatalogResolverError::InvalidCatalog { issue, .. } => {
            warn!(
                ?issue,
                "ERC-20 transfer asset catalog is internally inconsistent"
            );
            ApiError::internal_error()
        }
    }
}

fn push_unique_contract_address(
    contract_addresses: &mut Vec<String>,
    seen: &mut HashSet<String>,
    contract_address: String,
) {
    let contract_address = contract_address.to_ascii_lowercase();

    if seen.insert(contract_address.clone()) {
        contract_addresses.push(contract_address);
    }
}

fn enforce_token_filter_limit(
    contract_addresses: &[String],
    max_token_filters: u64,
) -> Result<(), ApiError> {
    let token_filter_count = u64::try_from(contract_addresses.len()).unwrap_or(u64::MAX);

    if token_filter_count > max_token_filters {
        Err(ApiError::too_many_token_filters())
    } else {
        Ok(())
    }
}
