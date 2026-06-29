use std::collections::HashSet;

use crate::adapters::postgres::global_assets::GlobalAssetRepository;
use crate::application::balances::catalog::{
    BalanceTargetResolution, CatalogBalanceTargetResolver,
};
use crate::application::filters::onchain_window::OnchainWindow;
use crate::application::filters::transfer_direction::TransferDirection;
use crate::domain::balance_catalog::{BalanceTargetKind, CatalogResolverError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Erc20TransferSearchInput {
    pub network_slug: String,
    pub address: String,
    pub direction: TransferDirection,
    pub tokens: Erc20TransferSearchTokenFilters,
    pub window: OnchainWindow,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Erc20TransferSearchTokenFilters {
    pub asset_slugs: Vec<String>,
    pub contract_addresses: Vec<String>,
}

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

#[derive(Debug, thiserror::Error)]
pub(crate) enum Erc20TransferSearchError {
    #[error("ERC-20 transfer asset catalog mapping is unavailable")]
    AssetContractMappingUnavailable,
    #[error("ERC-20 transfer asset catalog lookup failed: {0}")]
    Catalog(#[from] CatalogResolverError),
    #[error("ERC-20 transfer asset was not found")]
    AssetNotFound,
    #[error("ERC-20 transfer asset is not available on the requested network")]
    AssetNotAvailableOnNetwork,
    #[error("ERC-20 transfer asset is not an ERC-20 token on the requested network")]
    AssetNotErc20OnNetwork,
    #[error("too many ERC-20 transfer token filters were requested")]
    TooManyTokenFilters,
    #[error("invalid ERC-20 transfer catalog resolution: {0:?}")]
    InvalidCatalogResolution(Erc20TransferCatalogResolutionIssue),
    #[error("ERC-20 transfer extraction is unavailable")]
    ExtractionUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Erc20TransferCatalogResolutionIssue {
    ResolutionCountMismatch,
    ResolutionTargetMismatch,
}

pub(crate) async fn build_command(
    input: Erc20TransferSearchInput,
    repository: Option<GlobalAssetRepository>,
    max_token_filters: u64,
) -> Result<Erc20TransferSearchCommand, Erc20TransferSearchError> {
    let contract_addresses =
        resolve_token_filters(repository, &input.network_slug, input.tokens).await?;
    enforce_token_filter_limit(&contract_addresses, max_token_filters)?;

    Ok(Erc20TransferSearchCommand {
        network_slug: input.network_slug,
        address: input.address.to_ascii_lowercase(),
        direction: input.direction,
        tokens: Erc20TransferCommandTokenFilters { contract_addresses },
        window: input.window,
    })
}

pub(crate) async fn extraction_unavailable_placeholder(
    _command: Erc20TransferSearchCommand,
) -> Result<(), Erc20TransferSearchError> {
    Err(Erc20TransferSearchError::ExtractionUnavailable)
}

async fn resolve_token_filters(
    repository: Option<GlobalAssetRepository>,
    network_slug: &str,
    tokens: Erc20TransferSearchTokenFilters,
) -> Result<Vec<String>, Erc20TransferSearchError> {
    let mut contract_addresses = Vec::new();
    let mut seen = HashSet::new();

    if !tokens.asset_slugs.is_empty() {
        let repository =
            repository.ok_or(Erc20TransferSearchError::AssetContractMappingUnavailable)?;
        let resolver = CatalogBalanceTargetResolver::new(repository);
        let resolved_contracts = resolver
            .resolve_network(network_slug, &tokens.asset_slugs)
            .await?;
        let resolved_contracts = resolved_contract_addresses_from_catalog(
            network_slug,
            &tokens.asset_slugs,
            resolved_contracts,
        )?;

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
) -> Result<Vec<String>, Erc20TransferSearchError> {
    if resolutions.len() != requested_asset_slugs.len() {
        return Err(Erc20TransferSearchError::InvalidCatalogResolution(
            Erc20TransferCatalogResolutionIssue::ResolutionCountMismatch,
        ));
    }

    let mut contract_addresses = Vec::with_capacity(resolutions.len());

    for (requested_asset_slug, resolution) in requested_asset_slugs.iter().zip(resolutions) {
        match resolution {
            BalanceTargetResolution::Resolved(target) => {
                if target.network_slug != network_slug || target.asset_slug != *requested_asset_slug
                {
                    return Err(Erc20TransferSearchError::InvalidCatalogResolution(
                        Erc20TransferCatalogResolutionIssue::ResolutionTargetMismatch,
                    ));
                }

                match target.kind {
                    BalanceTargetKind::Erc20 { contract_address } => {
                        contract_addresses.push(contract_address);
                    }
                    BalanceTargetKind::Native => {
                        return Err(Erc20TransferSearchError::AssetNotErc20OnNetwork);
                    }
                }
            }
            BalanceTargetResolution::UnsupportedAsset { .. } => {
                return Err(Erc20TransferSearchError::AssetNotFound);
            }
            BalanceTargetResolution::UnsupportedNetwork { .. }
            | BalanceTargetResolution::UnsupportedPair { .. } => {
                return Err(Erc20TransferSearchError::AssetNotAvailableOnNetwork);
            }
            BalanceTargetResolution::UnsupportedTokenStandard { .. } => {
                return Err(Erc20TransferSearchError::AssetNotErc20OnNetwork);
            }
        }
    }

    Ok(contract_addresses)
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
) -> Result<(), Erc20TransferSearchError> {
    let token_filter_count = u64::try_from(contract_addresses.len()).unwrap_or(u64::MAX);

    if token_filter_count > max_token_filters {
        Err(Erc20TransferSearchError::TooManyTokenFilters)
    } else {
        Ok(())
    }
}
