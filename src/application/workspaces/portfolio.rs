use std::{collections::BTreeMap, sync::Arc};

use crate::{
    adapters::postgres::workspaces::WorkspaceMemberAddress,
    application::balances::{
        command::{GetBalancesCommand, MAX_ACCOUNTS, MAX_TOKENS},
        error::GetBalancesCommandError,
    },
    domain::{
        accounts::OnchainAccount, assets::token_selector::TokenSelector,
        canonical_registry::CanonicalRegistry, onchain_time::as_of::AsOf,
    },
};

const PORTFOLIO_QUOTE_CURRENCY: &str = "USD";

#[derive(Clone, Debug)]
pub(crate) struct WorkspaceBalanceResolutionPlanner {
    registry: Arc<CanonicalRegistry>,
}

impl WorkspaceBalanceResolutionPlanner {
    pub(crate) fn new(registry: Arc<CanonicalRegistry>) -> Self {
        Self { registry }
    }

    pub(crate) fn plan(
        &self,
        members: &[WorkspaceMemberAddress],
    ) -> Result<Vec<GetBalancesCommand>, GetBalancesCommandError> {
        let mut members_by_network = BTreeMap::<String, Vec<&WorkspaceMemberAddress>>::new();
        for member in members {
            members_by_network
                .entry(member.network_slug.clone())
                .or_default()
                .push(member);
        }

        let mut commands = Vec::new();
        for (network_slug, members) in members_by_network {
            let asset_slugs = self
                .registry
                .active_asset_slugs_mapped_to_network(&network_slug);
            if asset_slugs.is_empty() {
                continue;
            }

            let mut members = members;
            members.sort_by_cached_key(|member| {
                (
                    member.address.to_ascii_lowercase(),
                    member.public_id.as_str(),
                )
            });

            for asset_chunk in asset_slugs.chunks(MAX_TOKENS) {
                for member_chunk in members.chunks(MAX_ACCOUNTS) {
                    commands.push(GetBalancesCommand::try_new(
                        AsOf::Latest,
                        member_chunk
                            .iter()
                            .map(|member| OnchainAccount {
                                network_slug: member.network_slug.clone(),
                                address: member.address.clone(),
                                client_ref: member.client_ref.clone(),
                            })
                            .collect(),
                        PORTFOLIO_QUOTE_CURRENCY.to_string(),
                        TokenSelector {
                            asset_slugs: asset_chunk.to_vec(),
                            contract_addresses: Vec::new(),
                        },
                    )?);
                }
            }
        }

        Ok(commands)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use uuid::Uuid;

    use crate::{
        adapters::postgres::workspaces::WorkspaceMemberAddress,
        application::balances::{
            catalog::{BalanceTargetResolution, CatalogBalanceTargetResolver},
            command::{MAX_ACCOUNTS, MAX_TOKENS},
        },
        domain::{
            assets::balance_catalog::BalanceTargetKind,
            canonical_registry::{
                CanonicalAsset, CanonicalAssetChainMap, CanonicalNetwork, CanonicalRegistry,
                Catalog, CATALOG_VERSION,
            },
            onchain_time::as_of::AsOf,
        },
        test_utils::fixtures::registry::embedded_canonical_registry,
    };

    use super::WorkspaceBalanceResolutionPlanner;

    fn member(network_slug: &str, address: &str, public_id: &str) -> WorkspaceMemberAddress {
        WorkspaceMemberAddress {
            id: Uuid::new_v4(),
            public_id: public_id.to_string(),
            network_slug: network_slug.to_string(),
            address: address.to_string(),
            client_ref: Some(format!("ref-{public_id}")),
            labels: Vec::new(),
        }
    }

    fn planner() -> WorkspaceBalanceResolutionPlanner {
        WorkspaceBalanceResolutionPlanner::new(embedded_canonical_registry())
    }

    fn command_shape(
        command: &crate::application::balances::command::GetBalancesCommand,
    ) -> (Vec<String>, Vec<String>) {
        (
            command.tokens().asset_slugs.clone(),
            command
                .accounts()
                .iter()
                .map(|account| account.address.clone())
                .collect(),
        )
    }

    #[test]
    fn embedded_catalog_selects_only_active_mapped_assets_in_canonical_order() {
        let commands = planner()
            .plan(&[member(
                "eth-mainnet",
                "0x1111111111111111111111111111111111111111",
                "wma_eth",
            )])
            .unwrap();

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].as_of(), &AsOf::Latest);
        assert_eq!(commands[0].quote_currency(), "USD");
        assert!(commands[0].tokens().contract_addresses.is_empty());
        assert_eq!(
            commands[0].tokens().asset_slugs,
            vec![
                "ethereum",
                "usdc",
                "wrapped-bitcoin",
                "mantle",
                "aave",
                "agora-usd",
                "usds",
                "gho",
                "dai",
                "metapool-dao",
                "usdt",
                "usde",
                "wrapped-ether",
                "mantle-cmeth",
                "susde",
            ]
        );
        assert!(!commands[0]
            .tokens()
            .asset_slugs
            .contains(&"bitcoin".to_string()));
        assert!(!commands[0]
            .tokens()
            .asset_slugs
            .contains(&"gold".to_string()));
    }

    #[test]
    fn asset_slug_selectors_cover_native_and_erc20_catalog_targets() {
        let commands = planner()
            .plan(&[member(
                "base-mainnet",
                "0x1111111111111111111111111111111111111111",
                "wma_base",
            )])
            .unwrap();
        let resolver = CatalogBalanceTargetResolver::new(embedded_canonical_registry());
        let targets = resolver.resolve_network("base-mainnet", &commands[0].tokens().asset_slugs);

        assert!(matches!(
            &targets[0],
            BalanceTargetResolution::Resolved(target)
                if target.asset_slug == "ethereum" && matches!(target.kind, BalanceTargetKind::Native)
        ));
        assert!(matches!(
            targets.iter().find(|target| matches!(target, BalanceTargetResolution::Resolved(value) if value.asset_slug == "usdc")),
            Some(BalanceTargetResolution::Resolved(target))
                if matches!(target.kind, BalanceTargetKind::Erc20 { .. })
        ));
    }

    #[test]
    fn unmapped_assets_are_excluded_without_creating_commands_for_an_unknown_network() {
        let commands = planner()
            .plan(&[
                member(
                    "base-mainnet",
                    "0x1111111111111111111111111111111111111111",
                    "wma_base",
                ),
                member(
                    "unknown-mainnet",
                    "0x2222222222222222222222222222222222222222",
                    "wma_unknown",
                ),
            ])
            .unwrap();

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].accounts()[0].network_slug, "base-mainnet");
        assert!(!commands[0]
            .tokens()
            .asset_slugs
            .contains(&"dai".to_string()));
    }

    #[test]
    fn planning_is_stable_across_member_input_order() {
        let members = vec![
            member(
                "eth-mainnet",
                "0xcccccccccccccccccccccccccccccccccccccccc",
                "wma_c",
            ),
            member(
                "base-mainnet",
                "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "wma_b",
            ),
            member(
                "eth-mainnet",
                "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "wma_a",
            ),
        ];
        let forward = planner().plan(&members).unwrap();
        let mut reverse_members = members;
        reverse_members.reverse();
        let reverse = planner().plan(&reverse_members).unwrap();

        assert_eq!(forward.len(), 2);
        assert_eq!(
            forward.iter().map(command_shape).collect::<Vec<_>>(),
            reverse.iter().map(command_shape).collect::<Vec<_>>()
        );
        assert_eq!(forward[0].accounts()[0].network_slug, "base-mainnet");
        assert_eq!(
            forward[1]
                .accounts()
                .iter()
                .map(|account| account.address.as_str())
                .collect::<Vec<_>>(),
            vec![
                "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "0xcccccccccccccccccccccccccccccccccccccccc",
            ]
        );
    }

    #[test]
    fn partitions_at_command_limits_and_preserves_network_selector_member_order() {
        let planner = WorkspaceBalanceResolutionPlanner::new(expanded_registry(MAX_TOKENS + 1));
        let members = (0..=MAX_ACCOUNTS)
            .map(|index| {
                member(
                    "test-mainnet",
                    &format!("0x{index:040x}"),
                    &format!("wma_{index:03}"),
                )
            })
            .rev()
            .collect::<Vec<_>>();

        let commands = planner.plan(&members).unwrap();

        assert_eq!(commands.len(), 4);
        assert_eq!(commands[0].tokens().asset_slugs.len(), MAX_TOKENS);
        assert_eq!(commands[1].tokens().asset_slugs.len(), MAX_TOKENS);
        assert_eq!(commands[2].tokens().asset_slugs.len(), 1);
        assert_eq!(commands[3].tokens().asset_slugs.len(), 1);
        assert_eq!(commands[0].accounts().len(), MAX_ACCOUNTS);
        assert_eq!(commands[1].accounts().len(), 1);
        assert_eq!(commands[2].accounts().len(), MAX_ACCOUNTS);
        assert_eq!(commands[3].accounts().len(), 1);
        assert_eq!(
            commands[0].tokens().asset_slugs,
            commands[1].tokens().asset_slugs
        );
        assert_eq!(
            commands[2].tokens().asset_slugs,
            commands[3].tokens().asset_slugs
        );
        assert_ne!(
            commands[0].tokens().asset_slugs,
            commands[2].tokens().asset_slugs
        );
        assert!(commands.iter().all(|command| {
            command.accounts().len() <= MAX_ACCOUNTS
                && command.tokens().asset_slugs.len() <= MAX_TOKENS
                && command.accounts().len() * command.tokens().asset_slugs.len()
                    <= MAX_ACCOUNTS * MAX_TOKENS
        }));
    }

    #[test]
    fn a_hundred_member_workspace_emits_independent_valid_commands() {
        let members = (0..100)
            .map(|index| {
                member(
                    "eth-mainnet",
                    &format!("0x{index:040x}"),
                    &format!("wma_{index:03}"),
                )
            })
            .collect::<Vec<_>>();

        let commands = planner().plan(&members).unwrap();

        assert_eq!(commands.len(), 2);
        assert!(commands.iter().all(|command| {
            command.accounts().len() == MAX_ACCOUNTS
                && command.tokens().asset_slugs.len() == 15
                && command.quote_currency() == "USD"
                && command.as_of() == &AsOf::Latest
        }));
        assert_eq!(
            commands
                .iter()
                .flat_map(|command| command.accounts())
                .map(|account| account.address.as_str())
                .collect::<Vec<_>>(),
            (0..100)
                .map(|index| format!("0x{index:040x}"))
                .collect::<Vec<_>>()
        );
    }

    fn expanded_registry(asset_count: usize) -> Arc<CanonicalRegistry> {
        let assets = (0..asset_count)
            .map(|index| CanonicalAsset {
                slug: format!("asset-{index:02}"),
                symbol: format!("ASSET-{index:02}"),
                name: format!("Asset {index:02}"),
                asset_kind: "crypto".to_string(),
                category: Some("crypto".to_string()),
                canonical_path: format!("/assets/asset-{index:02}"),
                aliases: Vec::new(),
                metadata: json!({}),
                status: "active".to_string(),
                sort_order: index as i32,
            })
            .collect::<Vec<_>>();
        let mappings = (0..asset_count)
            .map(|index| {
                let address_index = index + 1;
                CanonicalAssetChainMap {
                    asset_slug: format!("asset-{index:02}"),
                    network_slug: "test-mainnet".to_string(),
                    is_native: false,
                    deployment_address: Some(format!("0x{address_index:040x}")),
                    deployment_block: Some(1),
                    decimals: Some(18),
                    token_standard: "erc20".to_string(),
                    metadata: json!({}),
                    status: "active".to_string(),
                    sort_order: index as i32,
                }
            })
            .collect::<Vec<_>>();
        Arc::new(
            CanonicalRegistry::from_catalog(Catalog {
                version: CATALOG_VERSION,
                assets,
                networks: vec![CanonicalNetwork {
                    slug: "test-mainnet".to_string(),
                    name: "Test Mainnet".to_string(),
                    family: "evm".to_string(),
                    chain_id: Some(10_001),
                    caip2: Some("eip155:10001".to_string()),
                    metadata: json!({}),
                    status: "active".to_string(),
                    sort_order: 1,
                }],
                asset_chain_maps: mappings,
            })
            .unwrap(),
        )
    }
}
