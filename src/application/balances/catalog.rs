use std::sync::Arc;

use crate::domain::{
    assets::balance_catalog::{BalanceTarget, BalanceTargetKind},
    canonical_registry::{CanonicalAssetChainMap, CanonicalRegistry},
};

#[derive(Clone, Debug)]
pub struct CatalogBalanceTargetResolver {
    registry: Arc<CanonicalRegistry>,
}

impl CatalogBalanceTargetResolver {
    pub fn new(registry: Arc<CanonicalRegistry>) -> Self {
        Self { registry }
    }

    pub fn resolve_network(
        &self,
        network_slug: &str,
        ordered_asset_slugs: &[String],
    ) -> Vec<BalanceTargetResolution> {
        self.registry
            .ordered_balance_targets(network_slug, ordered_asset_slugs)
            .into_iter()
            .map(
                |target| match (target.network, target.asset, target.mapping) {
                    (None, _, _) => BalanceTargetResolution::UnsupportedNetwork {
                        network_slug: network_slug.to_string(),
                        asset_slug: target.requested_asset_slug.to_string(),
                    },
                    (_, None, _) => BalanceTargetResolution::UnsupportedAsset {
                        network_slug: network_slug.to_string(),
                        asset_slug: target.requested_asset_slug.to_string(),
                    },
                    (_, _, None) => BalanceTargetResolution::UnsupportedPair {
                        network_slug: network_slug.to_string(),
                        asset_slug: target.requested_asset_slug.to_string(),
                    },
                    (Some(network), Some(asset), Some(mapping)) => {
                        if network.family != "evm" {
                            BalanceTargetResolution::UnsupportedNetwork {
                                network_slug: network_slug.to_string(),
                                asset_slug: target.requested_asset_slug.to_string(),
                            }
                        } else {
                            target_from_mapping(
                                network.slug.as_str(),
                                network.chain_id,
                                asset,
                                mapping,
                            )
                            .map(BalanceTargetResolution::Resolved)
                            .unwrap_or_else(|| {
                                BalanceTargetResolution::UnsupportedTokenStandard {
                                    network_slug: network_slug.to_string(),
                                    asset_slug: target.requested_asset_slug.to_string(),
                                }
                            })
                        }
                    }
                },
            )
            .collect()
    }

    pub fn resolve_evm_network(&self, network_slug: &str) -> Option<BalanceNetworkResolution> {
        let network = self.registry.network_by_slug(network_slug)?;
        if network.status != "active" || network.family != "evm" {
            return None;
        }
        network
            .chain_id
            .filter(|id| *id > 0)
            .map(|chain_id| BalanceNetworkResolution {
                network_slug: network.slug.clone(),
                chain_id,
            })
    }

    pub fn resolve_erc20_contracts(
        &self,
        network: &BalanceNetworkResolution,
        ordered_contract_addresses: &[String],
    ) -> Vec<ContractBalanceTargetResolution> {
        ordered_contract_addresses
            .iter()
            .map(|address| {
                let address = address.to_ascii_lowercase();
                self.registry
                    .erc20_metadata(&network.network_slug, &address)
                    .and_then(|metadata| {
                        (metadata.network.chain_id == Some(network.chain_id)).then(|| {
                            target_from_mapping(
                                &network.network_slug,
                                metadata.network.chain_id,
                                metadata.asset,
                                metadata.mapping,
                            )
                        })
                    })
                    .flatten()
                    .map(ContractBalanceTargetResolution::Resolved)
                    .unwrap_or(ContractBalanceTargetResolution::Unknown {
                        network_slug: network.network_slug.clone(),
                        chain_id: network.chain_id,
                        contract_address: address,
                    })
            })
            .collect()
    }
}

fn target_from_mapping(
    network_slug: &str,
    chain_id: Option<i64>,
    asset: &crate::domain::canonical_registry::CanonicalAsset,
    mapping: &CanonicalAssetChainMap,
) -> Option<BalanceTarget> {
    let chain_id = chain_id.filter(|id| *id > 0)?;
    let decimals = u8::try_from(mapping.decimals?).ok()?;
    let kind = if mapping.is_native && mapping.token_standard == "native" {
        BalanceTargetKind::Native
    } else if !mapping.is_native && mapping.token_standard == "erc20" {
        BalanceTargetKind::Erc20 {
            contract_address: mapping.deployment_address.clone()?,
        }
    } else {
        return None;
    };
    Some(BalanceTarget {
        network_slug: network_slug.to_string(),
        chain_id,
        asset_slug: asset.slug.clone(),
        symbol: asset.symbol.clone(),
        name: asset.name.clone(),
        decimals,
        pricing_asset_slug: asset.slug.clone(),
        kind,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BalanceTargetResolution {
    Resolved(BalanceTarget),
    UnsupportedNetwork {
        network_slug: String,
        asset_slug: String,
    },
    UnsupportedAsset {
        network_slug: String,
        asset_slug: String,
    },
    UnsupportedPair {
        network_slug: String,
        asset_slug: String,
    },
    UnsupportedTokenStandard {
        network_slug: String,
        asset_slug: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BalanceNetworkResolution {
    pub network_slug: String,
    pub chain_id: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContractBalanceTargetResolution {
    Resolved(BalanceTarget),
    Unknown {
        network_slug: String,
        chain_id: i64,
        contract_address: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::embedded_canonical_registry;

    #[tokio::test]
    async fn resolves_catalog_targets_without_postgres() {
        let resolver = CatalogBalanceTargetResolver::new(embedded_canonical_registry());
        let targets =
            resolver.resolve_network("eth-mainnet", &["ethereum".to_string(), "usdc".to_string()]);
        assert!(matches!(targets[0], BalanceTargetResolution::Resolved(_)));
        assert!(matches!(targets[1], BalanceTargetResolution::Resolved(_)));
    }

    #[tokio::test]
    async fn unknown_contract_stays_unknown_without_postgres() {
        let resolver = CatalogBalanceTargetResolver::new(embedded_canonical_registry());
        let network = resolver.resolve_evm_network("eth-mainnet").unwrap();
        let targets = resolver.resolve_erc20_contracts(
            &network,
            &["0x1111111111111111111111111111111111111111".to_string()],
        );
        assert!(matches!(
            targets[0],
            ContractBalanceTargetResolution::Unknown { .. }
        ));
    }
}
