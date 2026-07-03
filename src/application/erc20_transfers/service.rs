use std::{
    collections::{HashMap, HashSet},
    future::Future,
};

use crate::adapters::postgres::global_assets::GlobalAssetRepository;
use crate::application::balances::catalog::{
    BalanceTargetResolution, CatalogBalanceTargetResolver,
};
use crate::domain::assets::balance_catalog::{
    BalanceTargetKind, CatalogIntegrityIssue, CatalogResolverError,
};
use crate::domain::onchain_time::onchain_window::OnchainWindow;
use crate::domain::transfers::transfer_direction::TransferDirection;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Erc20TransferSearchInput {
    pub network_slug: String,
    pub address: String,
    pub direction: TransferDirection,
    pub window: OnchainWindow,
    pub asset_slugs: Vec<String>,
    pub contract_addresses: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Erc20TransferSearchTokenFilters {
    pub asset_slugs: Vec<String>,
    pub contract_addresses: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Erc20TransferSearchPlan {
    pub extraction_request: Erc20TransferExtractionRequest,
    pub requested_token_filters: Erc20TransferSearchTokenFilters,
    pub resolved_token_filters: Vec<ResolvedErc20TransferTokenFilter>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Erc20TransferExtractionRequest {
    pub network_slug: String,
    pub address: String,
    pub direction: TransferDirection,
    pub window: OnchainWindow,
    pub contract_addresses: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedErc20TransferTokenFilter {
    pub contract_address: String,
    pub asset_slug: Option<String>,
    pub symbol: Option<String>,
    pub decimals: Option<u8>,
    pub source: Erc20TransferTokenFilterSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Erc20TransferTokenFilterSource {
    AssetSlug,
    ContractAddress,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Erc20TransferSearchResult {
    pub plan: Erc20TransferSearchPlan,
    pub extraction: Erc20TransferExtractionResult,
    pub token_metadata: Vec<Erc20TransferTokenCatalogMetadata>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Erc20TransferTokenCatalogMetadata {
    pub contract_address: String,
    pub asset_slug: String,
    pub symbol: String,
    pub decimals: u8,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Erc20TransferExtractionResult {
    pub rows: Vec<Erc20TransferExtractionRow>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Erc20TransferExtractionRow {
    pub block_number: u64,
    pub tx_hash: String,
    pub log_index: u64,
    pub token: String,
    pub from: String,
    pub to: String,
    pub value: String,
}

pub(crate) trait Erc20TransferExtractor {
    fn search_erc20_transfers(
        &self,
        request: Erc20TransferExtractionRequest,
    ) -> impl Future<Output = Result<Erc20TransferExtractionResult, Erc20TransferExtractionError>> + Send;
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
    #[error("ERC-20 transfer window exceeds the public limit")]
    WindowTooLarge,
    #[error("ERC-20 transfer window is invalid")]
    InvalidWindow,
    #[error("ERC-20 transfer extraction is unavailable")]
    ExtractionUnavailable,
    #[error("ERC-20 transfer extraction timed out")]
    ExtractionTimeout,
    #[error("ERC-20 transfer upstream provider failed")]
    UpstreamProviderError,
    #[error("ERC-20 transfer upstream provider timed out")]
    UpstreamProviderTimeout,
    #[error("ERC-20 transfer extraction response was internally inconsistent")]
    InternalError,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Erc20TransferCatalogResolutionIssue {
    ResolutionCountMismatch,
    ResolutionTargetMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum Erc20TransferExtractionError {
    #[error("ERC-20 transfer window exceeds the public limit")]
    WindowTooLarge,
    #[error("ERC-20 transfer window is invalid")]
    InvalidWindow,
    #[error("ERC-20 transfer extraction is unavailable")]
    ExtractionUnavailable,
    #[error("ERC-20 transfer extraction timed out")]
    ExtractionTimeout,
    #[error("ERC-20 transfer upstream provider failed")]
    UpstreamProviderError,
    #[error("ERC-20 transfer upstream provider timed out")]
    UpstreamProviderTimeout,
    #[error("ERC-20 transfer extraction response was internally inconsistent")]
    InternalError,
}

#[allow(dead_code)]
pub(crate) async fn search_erc20_transfers<E>(
    input: Erc20TransferSearchInput,
    repository: Option<GlobalAssetRepository>,
    max_token_filters: u64,
    extractor: &E,
) -> Result<Erc20TransferSearchResult, Erc20TransferSearchError>
where
    E: Erc20TransferExtractor + Sync,
{
    let plan = build_search_plan(input, repository.clone(), max_token_filters).await?;

    execute_search_plan(plan, repository, extractor).await
}

pub(crate) async fn build_search_plan(
    input: Erc20TransferSearchInput,
    repository: Option<GlobalAssetRepository>,
    max_token_filters: u64,
) -> Result<Erc20TransferSearchPlan, Erc20TransferSearchError> {
    let requested_token_filters = Erc20TransferSearchTokenFilters {
        asset_slugs: input.asset_slugs,
        contract_addresses: input.contract_addresses,
    };
    let resolved_token_filters = resolve_token_filters(
        repository,
        &input.network_slug,
        requested_token_filters.clone(),
    )
    .await?;
    enforce_token_filter_limit(&resolved_token_filters, max_token_filters)?;

    let contract_addresses = resolved_token_filters
        .iter()
        .map(|filter| filter.contract_address.clone())
        .collect();

    Ok(Erc20TransferSearchPlan {
        extraction_request: Erc20TransferExtractionRequest {
            network_slug: input.network_slug,
            address: input.address.to_ascii_lowercase(),
            direction: input.direction,
            window: input.window,
            contract_addresses,
        },
        requested_token_filters,
        resolved_token_filters,
    })
}

pub(crate) async fn execute_search_plan<E>(
    mut plan: Erc20TransferSearchPlan,
    repository: Option<GlobalAssetRepository>,
    extractor: &E,
) -> Result<Erc20TransferSearchResult, Erc20TransferSearchError>
where
    E: Erc20TransferExtractor + Sync,
{
    let extraction = extractor
        .search_erc20_transfers(plan.extraction_request.clone())
        .await
        .map_err(Erc20TransferSearchError::from)?;
    let token_metadata = enrich_token_metadata(&mut plan, &extraction, repository).await?;

    Ok(Erc20TransferSearchResult {
        plan,
        extraction,
        token_metadata,
    })
}

async fn enrich_token_metadata(
    plan: &mut Erc20TransferSearchPlan,
    extraction: &Erc20TransferExtractionResult,
    repository: Option<GlobalAssetRepository>,
) -> Result<Vec<Erc20TransferTokenCatalogMetadata>, Erc20TransferSearchError> {
    let lookup_addresses = collect_metadata_lookup_addresses(plan, extraction);
    let mut metadata_by_contract = plan
        .resolved_token_filters
        .iter()
        .filter_map(metadata_from_resolved_filter)
        .map(|metadata| (metadata.contract_address.clone(), metadata))
        .collect::<HashMap<_, _>>();

    if let Some(repository) = repository {
        let rows = repository
            .load_erc20_token_catalog_rows(&plan.extraction_request.network_slug, &lookup_addresses)
            .await
            .map_err(CatalogResolverError::from)?;

        for row in rows {
            let decimals = row
                .decimals
                .and_then(|decimals| u8::try_from(decimals).ok())
                .ok_or_else(|| {
                    Erc20TransferSearchError::Catalog(CatalogResolverError::InvalidCatalog {
                        network_slug: plan.extraction_request.network_slug.clone(),
                        asset_slug: Some(row.asset_slug.clone()),
                        issue: CatalogIntegrityIssue::InvalidDecimals,
                    })
                })?;

            let metadata = Erc20TransferTokenCatalogMetadata {
                contract_address: row.contract_address.to_ascii_lowercase(),
                asset_slug: row.asset_slug,
                symbol: row.asset_symbol,
                decimals,
            };
            metadata_by_contract
                .entry(metadata.contract_address.clone())
                .or_insert(metadata);
        }
    }

    for token_filter in &mut plan.resolved_token_filters {
        let contract_address = token_filter.contract_address.to_ascii_lowercase();
        if let Some(metadata) = metadata_by_contract.get(&contract_address) {
            token_filter.asset_slug = Some(metadata.asset_slug.clone());
            token_filter.symbol = Some(metadata.symbol.clone());
            token_filter.decimals = Some(metadata.decimals);
        }
    }

    Ok(lookup_addresses
        .into_iter()
        .filter_map(|contract_address| metadata_by_contract.get(&contract_address).cloned())
        .collect())
}

fn collect_metadata_lookup_addresses(
    plan: &Erc20TransferSearchPlan,
    extraction: &Erc20TransferExtractionResult,
) -> Vec<String> {
    let mut addresses = Vec::new();
    let mut seen = HashSet::new();

    for token_filter in &plan.resolved_token_filters {
        push_unique_address(&mut addresses, &mut seen, &token_filter.contract_address);
    }

    for row in &extraction.rows {
        push_unique_address(&mut addresses, &mut seen, &row.token);
    }

    addresses
}

fn push_unique_address(addresses: &mut Vec<String>, seen: &mut HashSet<String>, address: &str) {
    let address = address.to_ascii_lowercase();
    if seen.insert(address.clone()) {
        addresses.push(address);
    }
}

fn metadata_from_resolved_filter(
    token_filter: &ResolvedErc20TransferTokenFilter,
) -> Option<Erc20TransferTokenCatalogMetadata> {
    Some(Erc20TransferTokenCatalogMetadata {
        contract_address: token_filter.contract_address.to_ascii_lowercase(),
        asset_slug: token_filter.asset_slug.clone()?,
        symbol: token_filter.symbol.clone()?,
        decimals: token_filter.decimals?,
    })
}

async fn resolve_token_filters(
    repository: Option<GlobalAssetRepository>,
    network_slug: &str,
    tokens: Erc20TransferSearchTokenFilters,
) -> Result<Vec<ResolvedErc20TransferTokenFilter>, Erc20TransferSearchError> {
    let mut resolved_token_filters = Vec::new();
    let mut seen = HashSet::new();

    if !tokens.asset_slugs.is_empty() {
        let repository =
            repository.ok_or(Erc20TransferSearchError::AssetContractMappingUnavailable)?;
        // Reused by ERC-20 transfer search to resolve public asset slugs into
        // network-specific ERC-20 contract addresses. The resolver is still
        // balance-named because it owns catalog-backed asset target resolution.
        let resolver = CatalogBalanceTargetResolver::new(repository);
        let resolved_asset_filters = resolver
            .resolve_network(network_slug, &tokens.asset_slugs)
            .await?;
        let resolved_asset_filters = resolved_token_filters_from_catalog(
            network_slug,
            &tokens.asset_slugs,
            resolved_asset_filters,
        )?;

        for token_filter in resolved_asset_filters {
            push_unique_token_filter(&mut resolved_token_filters, &mut seen, token_filter);
        }
    }

    for contract_address in tokens.contract_addresses {
        push_unique_token_filter(
            &mut resolved_token_filters,
            &mut seen,
            ResolvedErc20TransferTokenFilter {
                contract_address,
                asset_slug: None,
                symbol: None,
                decimals: None,
                source: Erc20TransferTokenFilterSource::ContractAddress,
            },
        );
    }

    Ok(resolved_token_filters)
}

fn resolved_token_filters_from_catalog(
    network_slug: &str,
    requested_asset_slugs: &[String],
    resolutions: Vec<BalanceTargetResolution>,
) -> Result<Vec<ResolvedErc20TransferTokenFilter>, Erc20TransferSearchError> {
    if resolutions.len() != requested_asset_slugs.len() {
        return Err(Erc20TransferSearchError::InvalidCatalogResolution(
            Erc20TransferCatalogResolutionIssue::ResolutionCountMismatch,
        ));
    }

    let mut token_filters = Vec::with_capacity(resolutions.len());

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
                        token_filters.push(ResolvedErc20TransferTokenFilter {
                            contract_address,
                            asset_slug: Some(target.asset_slug),
                            symbol: Some(target.symbol),
                            decimals: Some(target.decimals),
                            source: Erc20TransferTokenFilterSource::AssetSlug,
                        });
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

    Ok(token_filters)
}

fn push_unique_token_filter(
    token_filters: &mut Vec<ResolvedErc20TransferTokenFilter>,
    seen: &mut HashSet<String>,
    mut token_filter: ResolvedErc20TransferTokenFilter,
) {
    token_filter.contract_address = token_filter.contract_address.to_ascii_lowercase();

    if seen.insert(token_filter.contract_address.clone()) {
        token_filters.push(token_filter);
    }
}

fn enforce_token_filter_limit(
    token_filters: &[ResolvedErc20TransferTokenFilter],
    max_token_filters: u64,
) -> Result<(), Erc20TransferSearchError> {
    let token_filter_count = u64::try_from(token_filters.len()).unwrap_or(u64::MAX);

    if token_filter_count > max_token_filters {
        Err(Erc20TransferSearchError::TooManyTokenFilters)
    } else {
        Ok(())
    }
}

impl From<Erc20TransferExtractionError> for Erc20TransferSearchError {
    fn from(error: Erc20TransferExtractionError) -> Self {
        match error {
            Erc20TransferExtractionError::WindowTooLarge => Self::WindowTooLarge,
            Erc20TransferExtractionError::InvalidWindow => Self::InvalidWindow,
            Erc20TransferExtractionError::ExtractionUnavailable => Self::ExtractionUnavailable,
            Erc20TransferExtractionError::ExtractionTimeout => Self::ExtractionTimeout,
            Erc20TransferExtractionError::UpstreamProviderError => Self::UpstreamProviderError,
            Erc20TransferExtractionError::UpstreamProviderTimeout => Self::UpstreamProviderTimeout,
            Erc20TransferExtractionError::InternalError => Self::InternalError,
        }
    }
}
