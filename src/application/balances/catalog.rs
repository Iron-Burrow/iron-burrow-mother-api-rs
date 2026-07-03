use crate::adapters::postgres::balance_catalog::{BalanceCatalogRow, Erc20TokenCatalogRow};
use crate::adapters::postgres::global_assets::GlobalAssetRepository;
use crate::domain::assets::balance_catalog::{
    BalanceTarget, BalanceTargetKind, CatalogIntegrityIssue, CatalogResolverError,
};

#[derive(Clone, Debug)]
pub struct CatalogBalanceTargetResolver {
    repository: GlobalAssetRepository,
}

impl CatalogBalanceTargetResolver {
    pub fn new(repository: GlobalAssetRepository) -> Self {
        Self { repository }
    }

    pub async fn resolve_network(
        &self,
        network_slug: &str,
        ordered_asset_slugs: &[String],
    ) -> Result<Vec<BalanceTargetResolution>, CatalogResolverError> {
        let rows = self
            .repository
            .load_balance_catalog_rows(network_slug, ordered_asset_slugs)
            .await?;

        resolve_catalog_rows(network_slug, ordered_asset_slugs, &rows)
    }

    pub async fn resolve_evm_network(
        &self,
        network_slug: &str,
    ) -> Result<Option<BalanceNetworkResolution>, CatalogResolverError> {
        let Some(row) = self
            .repository
            .load_balance_network_catalog_row(network_slug)
            .await?
        else {
            return Ok(None);
        };

        if row.network_slug != network_slug || row.network_family != "evm" {
            return Ok(None);
        }

        let Some(chain_id) = row.network_chain_id.filter(|chain_id| *chain_id > 0) else {
            return Err(invalid_catalog(
                network_slug,
                None,
                CatalogIntegrityIssue::InvalidChainId,
            ));
        };

        Ok(Some(BalanceNetworkResolution {
            network_slug: row.network_slug,
            chain_id,
        }))
    }

    pub async fn resolve_erc20_contracts(
        &self,
        network: &BalanceNetworkResolution,
        ordered_contract_addresses: &[String],
    ) -> Result<Vec<ContractBalanceTargetResolution>, CatalogResolverError> {
        let rows = self
            .repository
            .load_erc20_token_catalog_rows(&network.network_slug, ordered_contract_addresses)
            .await?;

        resolve_contract_rows(network, ordered_contract_addresses, &rows)
    }
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

fn resolve_catalog_rows(
    requested_network_slug: &str,
    ordered_asset_slugs: &[String],
    rows: &[BalanceCatalogRow],
) -> Result<Vec<BalanceTargetResolution>, CatalogResolverError> {
    let mut resolutions = Vec::with_capacity(ordered_asset_slugs.len());

    for (index, requested_asset_slug) in ordered_asset_slugs.iter().enumerate() {
        let ordinal = i64::try_from(index + 1).unwrap_or(i64::MAX);
        let matching_rows = rows
            .iter()
            .filter(|row| row.ordinal == ordinal)
            .collect::<Vec<_>>();

        if matching_rows.is_empty() {
            return Err(invalid_catalog(
                requested_network_slug,
                Some(requested_asset_slug),
                CatalogIntegrityIssue::MissingLookupRow,
            ));
        }

        if matching_rows
            .iter()
            .any(|row| row.requested_asset_slug != *requested_asset_slug)
        {
            return Err(invalid_catalog(
                requested_network_slug,
                Some(requested_asset_slug),
                CatalogIntegrityIssue::UnexpectedLookupRow,
            ));
        }

        let first = matching_rows[0];
        let Some(network_slug) = first.network_slug.as_deref() else {
            resolutions.push(BalanceTargetResolution::UnsupportedNetwork {
                network_slug: requested_network_slug.to_string(),
                asset_slug: requested_asset_slug.clone(),
            });
            continue;
        };

        if network_slug != requested_network_slug
            || matching_rows
                .iter()
                .any(|row| row.network_slug.as_deref() != Some(network_slug))
        {
            return Err(invalid_catalog(
                requested_network_slug,
                Some(requested_asset_slug),
                CatalogIntegrityIssue::UnexpectedLookupRow,
            ));
        }

        if first.network_family.as_deref() != Some("evm") {
            resolutions.push(BalanceTargetResolution::UnsupportedNetwork {
                network_slug: requested_network_slug.to_string(),
                asset_slug: requested_asset_slug.clone(),
            });
            continue;
        }

        let Some(chain_id) = first.network_chain_id.filter(|chain_id| *chain_id > 0) else {
            return Err(invalid_catalog(
                requested_network_slug,
                None,
                CatalogIntegrityIssue::InvalidChainId,
            ));
        };

        let Some(asset_slug) = first.asset_slug.as_deref() else {
            resolutions.push(BalanceTargetResolution::UnsupportedAsset {
                network_slug: requested_network_slug.to_string(),
                asset_slug: requested_asset_slug.clone(),
            });
            continue;
        };

        if asset_slug != requested_asset_slug
            || matching_rows
                .iter()
                .any(|row| row.asset_slug.as_deref() != Some(asset_slug))
        {
            return Err(invalid_catalog(
                requested_network_slug,
                Some(requested_asset_slug),
                CatalogIntegrityIssue::UnexpectedLookupRow,
            ));
        }

        let concrete_rows = matching_rows
            .into_iter()
            .filter(|row| row.mapping_id.is_some())
            .collect::<Vec<_>>();

        if concrete_rows.is_empty() {
            resolutions.push(BalanceTargetResolution::UnsupportedPair {
                network_slug: requested_network_slug.to_string(),
                asset_slug: requested_asset_slug.clone(),
            });
            continue;
        }

        if concrete_rows.len() > 1 {
            return Err(invalid_catalog(
                requested_network_slug,
                Some(requested_asset_slug),
                CatalogIntegrityIssue::AmbiguousMapping,
            ));
        }

        let row = concrete_rows[0];
        let Some(is_native) = row.is_native else {
            return Err(invalid_catalog(
                requested_network_slug,
                Some(requested_asset_slug),
                CatalogIntegrityIssue::IncompleteMapping,
            ));
        };

        if !is_native && row.token_standard.as_deref() != Some("erc20") {
            if row.token_standard.as_deref() == Some("native") {
                return Err(invalid_catalog(
                    requested_network_slug,
                    Some(requested_asset_slug),
                    CatalogIntegrityIssue::ContradictoryNativeMapping,
                ));
            }

            resolutions.push(BalanceTargetResolution::UnsupportedTokenStandard {
                network_slug: requested_network_slug.to_string(),
                asset_slug: requested_asset_slug.clone(),
            });
            continue;
        }

        let decimals = row
            .decimals
            .and_then(|decimals| u8::try_from(decimals).ok())
            .ok_or_else(|| {
                invalid_catalog(
                    requested_network_slug,
                    Some(requested_asset_slug),
                    CatalogIntegrityIssue::InvalidDecimals,
                )
            })?;
        let symbol = row.asset_symbol.clone().ok_or_else(|| {
            invalid_catalog(
                requested_network_slug,
                Some(requested_asset_slug),
                CatalogIntegrityIssue::UnexpectedLookupRow,
            )
        })?;
        let name = row.asset_name.clone().ok_or_else(|| {
            invalid_catalog(
                requested_network_slug,
                Some(requested_asset_slug),
                CatalogIntegrityIssue::UnexpectedLookupRow,
            )
        })?;

        let kind = if is_native {
            if row.deployment_address.is_some() || row.token_standard.as_deref() != Some("native") {
                return Err(invalid_catalog(
                    requested_network_slug,
                    Some(requested_asset_slug),
                    CatalogIntegrityIssue::ContradictoryNativeMapping,
                ));
            }
            BalanceTargetKind::Native
        } else {
            let contract_address = row
                .deployment_address
                .as_deref()
                .filter(|address| is_evm_address(address))
                .map(str::to_ascii_lowercase)
                .ok_or_else(|| {
                    invalid_catalog(
                        requested_network_slug,
                        Some(requested_asset_slug),
                        CatalogIntegrityIssue::MalformedErc20Address,
                    )
                })?;
            BalanceTargetKind::Erc20 { contract_address }
        };

        resolutions.push(BalanceTargetResolution::Resolved(BalanceTarget {
            network_slug: network_slug.to_string(),
            chain_id,
            asset_slug: asset_slug.to_string(),
            symbol,
            name,
            decimals,
            pricing_asset_slug: asset_slug.to_string(),
            kind,
        }));
    }

    if rows
        .iter()
        .any(|row| row.ordinal < 1 || row.ordinal > ordered_asset_slugs.len() as i64)
    {
        return Err(invalid_catalog(
            requested_network_slug,
            None,
            CatalogIntegrityIssue::UnexpectedLookupRow,
        ));
    }

    Ok(resolutions)
}

fn resolve_contract_rows(
    network: &BalanceNetworkResolution,
    ordered_contract_addresses: &[String],
    rows: &[Erc20TokenCatalogRow],
) -> Result<Vec<ContractBalanceTargetResolution>, CatalogResolverError> {
    let mut rows_by_contract = std::collections::HashMap::new();
    for row in rows {
        if row.network_slug != network.network_slug
            || row.network_chain_id != Some(network.chain_id)
            || !is_evm_address(&row.contract_address)
        {
            return Err(invalid_catalog(
                &network.network_slug,
                Some(&row.asset_slug),
                CatalogIntegrityIssue::UnexpectedLookupRow,
            ));
        }
        rows_by_contract
            .entry(row.contract_address.to_ascii_lowercase())
            .or_insert(row);
    }

    ordered_contract_addresses
        .iter()
        .map(|contract_address| {
            let contract_address = contract_address.to_ascii_lowercase();
            let Some(row) = rows_by_contract.get(&contract_address) else {
                return Ok(ContractBalanceTargetResolution::Unknown {
                    network_slug: network.network_slug.clone(),
                    chain_id: network.chain_id,
                    contract_address,
                });
            };

            let decimals = row
                .decimals
                .and_then(|decimals| u8::try_from(decimals).ok())
                .ok_or_else(|| {
                    invalid_catalog(
                        &network.network_slug,
                        Some(&row.asset_slug),
                        CatalogIntegrityIssue::InvalidDecimals,
                    )
                })?;

            Ok(ContractBalanceTargetResolution::Resolved(BalanceTarget {
                network_slug: network.network_slug.clone(),
                chain_id: network.chain_id,
                asset_slug: row.asset_slug.clone(),
                symbol: row.asset_symbol.clone(),
                name: row.asset_name.clone(),
                decimals,
                pricing_asset_slug: row.asset_slug.clone(),
                kind: BalanceTargetKind::Erc20 { contract_address },
            }))
        })
        .collect()
}

fn invalid_catalog(
    network_slug: &str,
    asset_slug: Option<&str>,
    issue: CatalogIntegrityIssue,
) -> CatalogResolverError {
    CatalogResolverError::InvalidCatalog {
        network_slug: network_slug.to_string(),
        asset_slug: asset_slug.map(str::to_string),
        issue,
    }
}

fn is_evm_address(address: &str) -> bool {
    address.len() == 42
        && address.starts_with("0x")
        && address.as_bytes()[2..]
            .iter()
            .all(|character| character.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::postgres::global_assets::GlobalAssetRepository;
    use crate::test_utils::fixtures::global_assets::sample_assets;

    fn resolver() -> CatalogBalanceTargetResolver {
        CatalogBalanceTargetResolver::new(GlobalAssetRepository::in_memory(sample_assets()))
    }

    #[tokio::test]
    async fn resolves_native_and_erc20_targets_in_requested_order() {
        let native = resolver()
            .resolve_network("eth-mainnet", &["ethereum".to_string()])
            .await
            .unwrap();
        let erc20 = resolver()
            .resolve_network("base-mainnet", &["usdc".to_string()])
            .await
            .unwrap();

        assert_eq!(
            native,
            vec![BalanceTargetResolution::Resolved(BalanceTarget {
                network_slug: "eth-mainnet".to_string(),
                chain_id: 1,
                asset_slug: "ethereum".to_string(),
                symbol: "ETH".to_string(),
                name: "Ethereum".to_string(),
                decimals: 18,
                pricing_asset_slug: "ethereum".to_string(),
                kind: BalanceTargetKind::Native,
            })]
        );
        assert_eq!(
            erc20,
            vec![BalanceTargetResolution::Resolved(BalanceTarget {
                network_slug: "base-mainnet".to_string(),
                chain_id: 8453,
                asset_slug: "usdc".to_string(),
                symbol: "USDC".to_string(),
                name: "USD Coin".to_string(),
                decimals: 6,
                pricing_asset_slug: "usdc".to_string(),
                kind: BalanceTargetKind::Erc20 {
                    contract_address: "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913".to_string(),
                },
            })]
        );
    }

    #[tokio::test]
    async fn preserves_order_and_duplicate_assets() {
        let resolutions = resolver()
            .resolve_network(
                "eth-mainnet",
                &[
                    "usdc".to_string(),
                    "ethereum".to_string(),
                    "usdc".to_string(),
                ],
            )
            .await
            .unwrap();

        assert_eq!(resolutions.len(), 3);
        assert!(matches!(
            &resolutions[0],
            BalanceTargetResolution::Resolved(target) if target.asset_slug == "usdc"
        ));
        assert!(matches!(
            &resolutions[1],
            BalanceTargetResolution::Resolved(target) if target.asset_slug == "ethereum"
        ));
        assert_eq!(resolutions[0], resolutions[2]);
    }

    #[tokio::test]
    async fn distinguishes_unsupported_network_asset_and_pair() {
        let legacy_network = resolver()
            .resolve_network("base", &["usdc".to_string()])
            .await
            .unwrap();
        let non_evm_network = resolver()
            .resolve_network("bitcoin-mainnet", &["bitcoin".to_string()])
            .await
            .unwrap();
        let unknown_asset = resolver()
            .resolve_network("base-mainnet", &["missing".to_string()])
            .await
            .unwrap();
        let unsupported_pair = resolver()
            .resolve_network("mantle-mainnet", &["wrapped-bitcoin".to_string()])
            .await
            .unwrap();

        assert!(matches!(
            &legacy_network[0],
            BalanceTargetResolution::UnsupportedNetwork { .. }
        ));
        assert!(matches!(
            &non_evm_network[0],
            BalanceTargetResolution::UnsupportedNetwork { .. }
        ));
        assert!(matches!(
            &unknown_asset[0],
            BalanceTargetResolution::UnsupportedAsset { .. }
        ));
        assert!(matches!(
            &unsupported_pair[0],
            BalanceTargetResolution::UnsupportedPair { .. }
        ));

        let mixed_case_asset = resolver()
            .resolve_network("base-mainnet", &["USDC".to_string()])
            .await
            .unwrap();
        assert!(matches!(
            &mixed_case_asset[0],
            BalanceTargetResolution::UnsupportedAsset { .. }
        ));
    }

    #[tokio::test]
    async fn database_resolver_reads_canonical_targets_and_inactive_entries_as_unsupported() {
        let Some(pool) = crate::test_utils::postgres::migrated_pool().await else {
            return;
        };
        let resolver =
            CatalogBalanceTargetResolver::new(GlobalAssetRepository::database(pool.clone()));

        let targets = resolver
            .resolve_network(
                "base-mainnet",
                &[
                    "ethereum".to_string(),
                    "usdc".to_string(),
                    "bitso-mxn".to_string(),
                ],
            )
            .await
            .unwrap();

        assert!(matches!(
            &targets[0],
            BalanceTargetResolution::Resolved(BalanceTarget {
                kind: BalanceTargetKind::Native,
                chain_id: 8453,
                ..
            })
        ));
        assert!(matches!(
            &targets[1],
            BalanceTargetResolution::Resolved(BalanceTarget {
                kind: BalanceTargetKind::Erc20 { contract_address },
                ..
            }) if contract_address == "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913"
        ));
        assert!(matches!(
            &targets[2],
            BalanceTargetResolution::UnsupportedPair { .. }
        ));

        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let inactive_asset_slug = format!("inactive-asset-{suffix}");
        let inactive_network_slug = format!("inactive-network-{suffix}");

        sqlx::query(
            r#"
            insert into mother_api.global_asset (
              slug,
              symbol,
              name,
              canonical_path,
              status
            )
            values ($1, $2, $3, $4, 'inactive')
            "#,
        )
        .bind(&inactive_asset_slug)
        .bind(format!("IA{suffix}"))
        .bind("Inactive Balance Test Asset")
        .bind(format!("/assets/{inactive_asset_slug}"))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            insert into mother_api.network (
              slug,
              name,
              family,
              chain_id,
              status
            )
            values ($1, 'Inactive Balance Test Network', 'evm', 999999, 'inactive')
            "#,
        )
        .bind(&inactive_network_slug)
        .execute(&pool)
        .await
        .unwrap();

        let inactive_asset = resolver
            .resolve_network("base-mainnet", std::slice::from_ref(&inactive_asset_slug))
            .await
            .unwrap();
        let inactive_network = resolver
            .resolve_network(&inactive_network_slug, &["usdc".to_string()])
            .await
            .unwrap();

        assert!(matches!(
            &inactive_asset[0],
            BalanceTargetResolution::UnsupportedAsset { .. }
        ));
        assert!(matches!(
            &inactive_network[0],
            BalanceTargetResolution::UnsupportedNetwork { .. }
        ));

        sqlx::query("delete from mother_api.global_asset where slug = $1")
            .bind(&inactive_asset_slug)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("delete from mother_api.network where slug = $1")
            .bind(&inactive_network_slug)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[test]
    fn normalizes_valid_erc20_addresses_to_lowercase() {
        assert!(is_evm_address("0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48"));
        assert_eq!(
            "0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48".to_ascii_lowercase(),
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
        );
    }

    #[test]
    fn rejects_malformed_erc20_addresses() {
        for address in [
            "a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            "0xa0b8",
            "0xg0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
        ] {
            assert!(!is_evm_address(address));
        }
    }

    #[test]
    fn reports_ambiguous_and_malformed_catalog_rows() {
        let requested_assets = vec!["usdc".to_string()];
        let valid = balance_row();

        let ambiguous = resolve_catalog_rows(
            "base-mainnet",
            &requested_assets,
            &[
                valid.clone(),
                BalanceCatalogRow {
                    mapping_id: Some("mapping-2".to_string()),
                    ..valid.clone()
                },
            ],
        )
        .unwrap_err();
        assert!(matches!(
            ambiguous,
            CatalogResolverError::InvalidCatalog {
                issue: CatalogIntegrityIssue::AmbiguousMapping,
                ..
            }
        ));

        let missing_decimals = resolve_catalog_rows(
            "base-mainnet",
            &requested_assets,
            &[BalanceCatalogRow {
                decimals: None,
                ..valid.clone()
            }],
        )
        .unwrap_err();
        assert!(matches!(
            missing_decimals,
            CatalogResolverError::InvalidCatalog {
                issue: CatalogIntegrityIssue::InvalidDecimals,
                ..
            }
        ));

        let contradictory_native = resolve_catalog_rows(
            "base-mainnet",
            &requested_assets,
            &[BalanceCatalogRow {
                is_native: Some(true),
                token_standard: Some("native".to_string()),
                ..valid.clone()
            }],
        )
        .unwrap_err();
        assert!(matches!(
            contradictory_native,
            CatalogResolverError::InvalidCatalog {
                issue: CatalogIntegrityIssue::ContradictoryNativeMapping,
                ..
            }
        ));

        let malformed_address = resolve_catalog_rows(
            "base-mainnet",
            &requested_assets,
            &[BalanceCatalogRow {
                deployment_address: Some("0xnot-an-address".to_string()),
                ..valid
            }],
        )
        .unwrap_err();
        assert!(matches!(
            malformed_address,
            CatalogResolverError::InvalidCatalog {
                issue: CatalogIntegrityIssue::MalformedErc20Address,
                ..
            }
        ));
    }

    #[test]
    fn active_non_erc20_mapping_is_an_unsupported_token_standard() {
        let resolution = resolve_catalog_rows(
            "base-mainnet",
            &["usdc".to_string()],
            &[BalanceCatalogRow {
                token_standard: Some("erc721".to_string()),
                ..balance_row()
            }],
        )
        .unwrap();

        assert!(matches!(
            &resolution[0],
            BalanceTargetResolution::UnsupportedTokenStandard { .. }
        ));
    }

    fn balance_row() -> BalanceCatalogRow {
        BalanceCatalogRow {
            ordinal: 1,
            requested_asset_slug: "usdc".to_string(),
            network_slug: Some("base-mainnet".to_string()),
            network_family: Some("evm".to_string()),
            network_chain_id: Some(8453),
            asset_slug: Some("usdc".to_string()),
            asset_symbol: Some("USDC".to_string()),
            asset_name: Some("USD Coin".to_string()),
            mapping_id: Some("mapping-1".to_string()),
            is_native: Some(false),
            deployment_address: Some("0x833589fcd6edb6e08f4c7c32d4f71b54bda02913".to_string()),
            decimals: Some(6),
            token_standard: Some("erc20".to_string()),
        }
    }
}
