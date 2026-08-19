use std::{collections::BTreeMap, sync::Arc, time::SystemTime};

use crate::{
    adapters::postgres::workspaces::{Workspace, WorkspaceMemberAddress},
    application::balances::{
        command::{GetBalancesCommand, MAX_ACCOUNTS, MAX_TOKENS},
        decimal::{add_unsigned_decimals, DecimalError},
        error::GetBalancesCommandError,
        quote::LatestPriceQuotes,
        result::{BalanceEvidence, BalanceItemOutcome, BalanceQuoteOutcome, GetBalancesResult},
        service::BalanceSnapshotService,
    },
    domain::{
        accounts::OnchainAccount, assets::token_selector::TokenSelector,
        canonical_registry::CanonicalRegistry, onchain_time::as_of::AsOf,
    },
};

const PORTFOLIO_QUOTE_CURRENCY: &str = "USD";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PortfolioObservationStatus {
    Complete,
    Partial,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CurrentWorkspacePortfolio {
    pub(crate) workspace: Workspace,
    pub(crate) resolved_at: SystemTime,
    pub(crate) quote_currency: String,
    pub(crate) members: Vec<MemberPortfolioObservation>,
    pub(crate) assets: Vec<AggregatedAssetObservation>,
    pub(crate) known_value: String,
    pub(crate) valuation_status: PortfolioObservationStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MemberPortfolioObservation {
    pub(crate) member: WorkspaceMemberAddress,
    pub(crate) contributions: Vec<PortfolioContribution>,
    pub(crate) observation_status: PortfolioObservationStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AggregatedAssetObservation {
    pub(crate) asset_slug: String,
    pub(crate) total_amount: Option<String>,
    pub(crate) known_value: String,
    pub(crate) contributions: Vec<PortfolioContribution>,
    pub(crate) valuation_status: PortfolioObservationStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PortfolioContribution {
    pub(crate) member_id: String,
    pub(crate) network_slug: String,
    pub(crate) address: String,
    pub(crate) labels: Vec<String>,
    pub(crate) evidence: Option<BalanceEvidence>,
    pub(crate) outcome: PortfolioContributionOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PortfolioContributionOutcome {
    Balance(BalanceItemOutcome),
    CommandUnavailable { asset_slug: String },
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum WorkspacePortfolioResolverError {
    #[error("portfolio balance planning failed: {0:?}")]
    Planning(GetBalancesCommandError),
    #[error("portfolio balance command did not match its result")]
    InconsistentBalanceResult,
    #[error("portfolio balance result lacked canonical asset identity")]
    MissingCanonicalAssetIdentity,
    #[error("portfolio balance result named an unknown canonical asset")]
    UnknownCanonicalAsset,
    #[error("portfolio balance decimal was invalid")]
    InvalidDecimal,
}

#[derive(Clone, Debug)]
pub(crate) struct WorkspacePortfolioResolver<Q> {
    registry: Arc<CanonicalRegistry>,
    planner: WorkspaceBalanceResolutionPlanner,
    balance_service: BalanceSnapshotService<Q>,
}

impl<Q> WorkspacePortfolioResolver<Q>
where
    Q: LatestPriceQuotes,
{
    pub(crate) fn new(
        registry: Arc<CanonicalRegistry>,
        balance_service: BalanceSnapshotService<Q>,
    ) -> Self {
        Self {
            planner: WorkspaceBalanceResolutionPlanner::new(registry.clone()),
            registry,
            balance_service,
        }
    }

    pub(crate) async fn resolve(
        &self,
        workspace: Workspace,
        members: Vec<WorkspaceMemberAddress>,
    ) -> Result<CurrentWorkspacePortfolio, WorkspacePortfolioResolverError> {
        let commands = self
            .planner
            .plan(&members)
            .map_err(WorkspacePortfolioResolverError::Planning)?;
        let mut outcomes = Vec::with_capacity(commands.len());

        for command in &commands {
            outcomes.push(match self.balance_service.resolve(command.clone()).await {
                Ok(result) => CommandResolution::Resolved(result),
                Err(_) => CommandResolution::Unavailable,
            });
        }

        Self::compose(
            &self.registry,
            workspace,
            members,
            &commands,
            outcomes,
            SystemTime::now(),
        )
    }

    fn compose(
        registry: &CanonicalRegistry,
        workspace: Workspace,
        members: Vec<WorkspaceMemberAddress>,
        commands: &[GetBalancesCommand],
        outcomes: Vec<CommandResolution>,
        resolved_at: SystemTime,
    ) -> Result<CurrentWorkspacePortfolio, WorkspacePortfolioResolverError> {
        if commands.len() != outcomes.len() {
            return Err(WorkspacePortfolioResolverError::InconsistentBalanceResult);
        }

        let mut members = members;
        members.sort_by(member_order);
        let member_lookup = members
            .iter()
            .map(|member| (member_key(member), member))
            .collect::<BTreeMap<_, _>>();
        let mut contributions_by_member = members
            .iter()
            .map(|member| (member_key(member), Vec::new()))
            .collect::<BTreeMap<_, Vec<PortfolioContribution>>>();

        for (command, outcome) in commands.iter().zip(outcomes) {
            if !command.tokens().contract_addresses.is_empty() {
                return Err(WorkspacePortfolioResolverError::InconsistentBalanceResult);
            }

            match outcome {
                CommandResolution::Resolved(result) => {
                    Self::append_balance_result(
                        command,
                        result,
                        &member_lookup,
                        &mut contributions_by_member,
                    )?;
                }
                CommandResolution::Unavailable => {
                    for account in command.accounts() {
                        let member = member_lookup
                            .get(&account_key(&account.network_slug, &account.address))
                            .ok_or(WorkspacePortfolioResolverError::InconsistentBalanceResult)?;
                        let contributions = contributions_by_member
                            .get_mut(&member_key(member))
                            .expect("every Workspace member has a contribution collection");
                        for asset_slug in &command.tokens().asset_slugs {
                            contributions
                                .push(command_unavailable_contribution(member, asset_slug));
                        }
                    }
                }
            }
        }

        let mut all_contributions = Vec::new();
        let mut member_observations = Vec::with_capacity(members.len());
        for member in members {
            let mut contributions = contributions_by_member
                .remove(&member_key(&member))
                .expect("every Workspace member has a contribution collection");
            contributions.sort_by(contribution_order);
            let observation_status = balance_status(&contributions);
            all_contributions.extend(contributions.iter().cloned());
            member_observations.push(MemberPortfolioObservation {
                member,
                contributions,
                observation_status,
            });
        }

        let mut contributions_by_asset = BTreeMap::<String, Vec<PortfolioContribution>>::new();
        for contribution in &all_contributions {
            let Some(asset_slug) = canonical_asset_slug(contribution)? else {
                continue;
            };
            if registry.asset_by_slug(asset_slug).is_none() {
                return Err(WorkspacePortfolioResolverError::UnknownCanonicalAsset);
            }
            contributions_by_asset
                .entry(asset_slug.to_string())
                .or_default()
                .push(contribution.clone());
        }

        let mut assets = contributions_by_asset
            .into_iter()
            .map(|(asset_slug, contributions)| aggregate_asset(asset_slug, contributions))
            .collect::<Result<Vec<_>, _>>()?;
        assets.sort_by(|left, right| canonical_asset_order(registry, left, right));
        let known_value =
            sum_decimal_values(assets.iter().map(|asset| asset.known_value.as_str()))?;

        let valuation_status = if member_observations.is_empty() {
            PortfolioObservationStatus::Complete
        } else if balance_status(&all_contributions) == PortfolioObservationStatus::Unavailable {
            PortfolioObservationStatus::Unavailable
        } else if balance_status(&all_contributions) == PortfolioObservationStatus::Partial
            || assets
                .iter()
                .any(|asset| asset.valuation_status != PortfolioObservationStatus::Complete)
        {
            PortfolioObservationStatus::Partial
        } else {
            PortfolioObservationStatus::Complete
        };

        Ok(CurrentWorkspacePortfolio {
            workspace,
            resolved_at,
            quote_currency: PORTFOLIO_QUOTE_CURRENCY.to_string(),
            members: member_observations,
            assets,
            known_value,
            valuation_status,
        })
    }

    fn append_balance_result(
        command: &GetBalancesCommand,
        result: GetBalancesResult,
        member_lookup: &BTreeMap<MemberKey, &WorkspaceMemberAddress>,
        contributions_by_member: &mut BTreeMap<MemberKey, Vec<PortfolioContribution>>,
    ) -> Result<(), WorkspacePortfolioResolverError> {
        if result.quote_currency != PORTFOLIO_QUOTE_CURRENCY
            || result.requested_token_count != command.tokens().len()
            || result.accounts.len() != command.accounts().len()
        {
            return Err(WorkspacePortfolioResolverError::InconsistentBalanceResult);
        }

        for (expected, actual) in command.accounts().iter().zip(result.accounts) {
            if account_key(&expected.network_slug, &expected.address)
                != account_key(&actual.account.network_slug, &actual.account.address)
                || actual.items.len() != command.tokens().len()
            {
                return Err(WorkspacePortfolioResolverError::InconsistentBalanceResult);
            }
            let member = member_lookup
                .get(&account_key(
                    &actual.account.network_slug,
                    &actual.account.address,
                ))
                .ok_or(WorkspacePortfolioResolverError::InconsistentBalanceResult)?;
            let contributions = contributions_by_member
                .get_mut(&member_key(member))
                .expect("every Workspace member has a contribution collection");
            for outcome in actual.items {
                contributions.push(PortfolioContribution {
                    member_id: member.public_id.clone(),
                    network_slug: member.network_slug.clone(),
                    address: member.address.clone(),
                    labels: member.labels.clone(),
                    evidence: actual.evidence.clone(),
                    outcome: PortfolioContributionOutcome::Balance(outcome),
                });
            }
        }

        Ok(())
    }
}

type MemberKey = (String, String);

enum CommandResolution {
    Resolved(GetBalancesResult),
    Unavailable,
}

fn member_key(member: &WorkspaceMemberAddress) -> MemberKey {
    account_key(&member.network_slug, &member.address)
}

fn account_key(network_slug: &str, address: &str) -> MemberKey {
    (
        network_slug.to_ascii_lowercase(),
        address.to_ascii_lowercase(),
    )
}

fn member_order(
    left: &WorkspaceMemberAddress,
    right: &WorkspaceMemberAddress,
) -> std::cmp::Ordering {
    (
        left.network_slug.as_str(),
        left.address.to_ascii_lowercase(),
        left.public_id.as_str(),
    )
        .cmp(&(
            right.network_slug.as_str(),
            right.address.to_ascii_lowercase(),
            right.public_id.as_str(),
        ))
}

fn command_unavailable_contribution(
    member: &WorkspaceMemberAddress,
    asset_slug: &str,
) -> PortfolioContribution {
    PortfolioContribution {
        member_id: member.public_id.clone(),
        network_slug: member.network_slug.clone(),
        address: member.address.clone(),
        labels: member.labels.clone(),
        evidence: None,
        outcome: PortfolioContributionOutcome::CommandUnavailable {
            asset_slug: asset_slug.to_string(),
        },
    }
}

fn contribution_order(
    left: &PortfolioContribution,
    right: &PortfolioContribution,
) -> std::cmp::Ordering {
    contribution_asset_slug(left)
        .cmp(&contribution_asset_slug(right))
        .then_with(|| left.address.cmp(&right.address))
}

fn contribution_asset_slug(contribution: &PortfolioContribution) -> Option<&str> {
    match &contribution.outcome {
        PortfolioContributionOutcome::Balance(BalanceItemOutcome::Resolved { target, .. })
        | PortfolioContributionOutcome::Balance(BalanceItemOutcome::Failed { target, .. }) => {
            target.asset_slug.as_deref()
        }
        PortfolioContributionOutcome::Balance(BalanceItemOutcome::Skipped {
            asset_slug, ..
        })
        | PortfolioContributionOutcome::CommandUnavailable { asset_slug } => Some(asset_slug),
    }
}

fn canonical_asset_slug(
    contribution: &PortfolioContribution,
) -> Result<Option<&str>, WorkspacePortfolioResolverError> {
    match &contribution.outcome {
        PortfolioContributionOutcome::Balance(BalanceItemOutcome::Resolved { target, .. }) => {
            target
                .asset_slug
                .as_deref()
                .map(Some)
                .ok_or(WorkspacePortfolioResolverError::MissingCanonicalAssetIdentity)
        }
        _ => Ok(None),
    }
}

fn balance_status(contributions: &[PortfolioContribution]) -> PortfolioObservationStatus {
    let resolved = contributions
        .iter()
        .filter(|contribution| {
            matches!(
                contribution.outcome,
                PortfolioContributionOutcome::Balance(BalanceItemOutcome::Resolved { .. })
            )
        })
        .count();

    if resolved == 0 {
        PortfolioObservationStatus::Unavailable
    } else if resolved == contributions.len() {
        PortfolioObservationStatus::Complete
    } else {
        PortfolioObservationStatus::Partial
    }
}

fn aggregate_asset(
    asset_slug: String,
    contributions: Vec<PortfolioContribution>,
) -> Result<AggregatedAssetObservation, WorkspacePortfolioResolverError> {
    let amounts = contributions
        .iter()
        .filter_map(resolved_amount)
        .collect::<Vec<_>>();
    let total_amount = (amounts.len() == contributions.len())
        .then(|| sum_decimal_values(amounts.into_iter()))
        .transpose()?;
    let quote_values = contributions
        .iter()
        .filter_map(available_usd_quote_value)
        .collect::<Vec<_>>();
    let known_value = sum_decimal_values(quote_values.iter().copied())?;
    let valuation_status = if total_amount.is_some() && quote_values.len() == contributions.len() {
        PortfolioObservationStatus::Complete
    } else {
        PortfolioObservationStatus::Partial
    };

    Ok(AggregatedAssetObservation {
        asset_slug,
        total_amount,
        known_value,
        contributions,
        valuation_status,
    })
}

fn resolved_amount(contribution: &PortfolioContribution) -> Option<&str> {
    match &contribution.outcome {
        PortfolioContributionOutcome::Balance(BalanceItemOutcome::Resolved { amount, .. }) => {
            amount.as_deref()
        }
        _ => None,
    }
}

fn available_usd_quote_value(contribution: &PortfolioContribution) -> Option<&str> {
    match &contribution.outcome {
        PortfolioContributionOutcome::Balance(BalanceItemOutcome::Resolved {
            quote:
                BalanceQuoteOutcome::Available {
                    currency, value, ..
                },
            ..
        }) if currency.eq_ignore_ascii_case(PORTFOLIO_QUOTE_CURRENCY) => Some(value),
        _ => None,
    }
}

fn sum_decimal_values<'a>(
    values: impl IntoIterator<Item = &'a str>,
) -> Result<String, WorkspacePortfolioResolverError> {
    values.into_iter().try_fold("0".to_string(), |sum, value| {
        add_unsigned_decimals(&sum, value)
            .map_err(|_: DecimalError| WorkspacePortfolioResolverError::InvalidDecimal)
    })
}

fn canonical_asset_order(
    registry: &CanonicalRegistry,
    left: &AggregatedAssetObservation,
    right: &AggregatedAssetObservation,
) -> std::cmp::Ordering {
    let left = registry
        .asset_by_slug(&left.asset_slug)
        .expect("aggregated portfolio assets must be canonical");
    let right = registry
        .asset_by_slug(&right.asset_slug)
        .expect("aggregated portfolio assets must be canonical");
    (
        left.sort_order,
        left.symbol.to_ascii_lowercase(),
        left.slug.as_str(),
    )
        .cmp(&(
            right.sort_order,
            right.symbol.to_ascii_lowercase(),
            right.slug.as_str(),
        ))
}

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
    use std::{collections::HashMap, sync::Arc, time::SystemTime};

    use serde_json::json;
    use uuid::Uuid;

    use crate::{
        adapters::postgres::workspaces::{Workspace, WorkspaceMemberAddress},
        application::balances::{
            catalog::{BalanceTargetResolution, CatalogBalanceTargetResolver},
            command::{GetBalancesCommand, MAX_ACCOUNTS, MAX_TOKENS},
            error::BalanceItemErrorCode,
            quote::{LatestPriceQuotes, PriceQuoteError, PriceQuoteResolution},
            result::{
                BalanceEvidence, BalanceItemOutcome, BalanceQuoteOutcome, BalanceTokenSelector,
                BalancesAccountResult, GetBalancesResult, ResolvedBalanceTarget,
            },
            service::BalanceSnapshotService,
        },
        domain::{
            accounts::OnchainAccount,
            assets::balance_catalog::BalanceTargetKind,
            assets::token_selector::TokenSelector,
            canonical_registry::{
                CanonicalAsset, CanonicalAssetChainMap, CanonicalNetwork, CanonicalRegistry,
                Catalog, CATALOG_VERSION,
            },
            onchain_time::as_of::AsOf,
        },
        test_utils::fixtures::registry::embedded_canonical_registry,
    };

    use super::{
        CommandResolution, PortfolioContributionOutcome, PortfolioObservationStatus,
        WorkspaceBalanceResolutionPlanner, WorkspacePortfolioResolver,
    };

    #[derive(Clone, Debug)]
    struct NoopQuoteReader;

    impl LatestPriceQuotes for NoopQuoteReader {
        async fn latest_quotes(
            &self,
            _: &[String],
            _: &str,
        ) -> Result<HashMap<String, PriceQuoteResolution>, PriceQuoteError> {
            unreachable!("the portfolio tests do not call the quote reader directly")
        }
    }

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

    fn workspace() -> Workspace {
        Workspace {
            id: Uuid::new_v4(),
            public_id: "wsp_portfolio".to_string(),
            name: "Portfolio".to_string(),
            description: None,
            status: "active".to_string(),
        }
    }

    fn command(network_slug: &str, address: &str, asset_slugs: &[&str]) -> GetBalancesCommand {
        GetBalancesCommand::try_new(
            AsOf::Latest,
            vec![OnchainAccount {
                network_slug: network_slug.to_string(),
                address: address.to_string(),
                client_ref: None,
            }],
            "USD".to_string(),
            TokenSelector {
                asset_slugs: asset_slugs.iter().map(|slug| (*slug).to_string()).collect(),
                contract_addresses: Vec::new(),
            },
        )
        .unwrap()
    }

    fn target(network_slug: &str, asset_slug: &str) -> ResolvedBalanceTarget {
        ResolvedBalanceTarget {
            selector: BalanceTokenSelector::AssetSlug(asset_slug.to_string()),
            network_slug: network_slug.to_string(),
            chain_id: if network_slug == "base-mainnet" {
                8453
            } else {
                1
            },
            asset_slug: Some(asset_slug.to_string()),
            symbol: Some(asset_slug.to_ascii_uppercase()),
            name: Some(asset_slug.to_string()),
            decimals: Some(6),
            pricing_asset_slug: Some(asset_slug.to_string()),
            kind: BalanceTargetKind::Erc20 {
                contract_address: "0x1111111111111111111111111111111111111111".to_string(),
            },
        }
    }

    fn evidence(network_slug: &str, observed_at: &str) -> BalanceEvidence {
        BalanceEvidence {
            network_slug: network_slug.to_string(),
            observed_at: observed_at.to_string(),
            block_number: "1".to_string(),
            block_hash: "0xabc".to_string(),
            block_timestamp: observed_at.to_string(),
        }
    }

    fn resolved_item(
        network_slug: &str,
        asset_slug: &str,
        amount: &str,
        quote: BalanceQuoteOutcome,
    ) -> BalanceItemOutcome {
        BalanceItemOutcome::Resolved {
            target: target(network_slug, asset_slug),
            raw_amount: "0".to_string(),
            amount: Some(amount.to_string()),
            quote,
        }
    }

    fn available_quote(value: &str, price_as_of: &str) -> BalanceQuoteOutcome {
        BalanceQuoteOutcome::Available {
            currency: "USD".to_string(),
            unit_price: "1".to_string(),
            value: value.to_string(),
            price_as_of: price_as_of.to_string(),
        }
    }

    fn result(
        command: &GetBalancesCommand,
        evidence: BalanceEvidence,
        items: Vec<BalanceItemOutcome>,
    ) -> GetBalancesResult {
        GetBalancesResult {
            as_of: AsOf::Latest,
            quote_currency: "USD".to_string(),
            requested_token_count: command.tokens().len(),
            accounts: vec![BalancesAccountResult {
                account: command.accounts()[0].clone(),
                evidence: Some(evidence),
                items,
            }],
        }
    }

    fn compose(
        members: Vec<WorkspaceMemberAddress>,
        commands: &[GetBalancesCommand],
        outcomes: Vec<CommandResolution>,
    ) -> super::CurrentWorkspacePortfolio {
        WorkspacePortfolioResolver::<NoopQuoteReader>::compose(
            &embedded_canonical_registry(),
            workspace(),
            members,
            commands,
            outcomes,
            SystemTime::UNIX_EPOCH,
        )
        .unwrap()
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
    fn composes_canonical_assets_with_member_network_and_quote_provenance() {
        let mut eth_member = member(
            "eth-mainnet",
            "0x1111111111111111111111111111111111111111",
            "wma_eth",
        );
        eth_member.labels = vec!["Treasury".to_string()];
        let base_member = member(
            "base-mainnet",
            "0x2222222222222222222222222222222222222222",
            "wma_base",
        );
        let eth_command = command("eth-mainnet", &eth_member.address, &["usdc"]);
        let base_command = command("base-mainnet", &base_member.address, &["usdc"]);
        let portfolio = compose(
            vec![eth_member.clone(), base_member.clone()],
            &[eth_command.clone(), base_command.clone()],
            vec![
                CommandResolution::Resolved(result(
                    &eth_command,
                    evidence("eth-mainnet", "2026-08-19T10:00:00Z"),
                    vec![resolved_item(
                        "eth-mainnet",
                        "usdc",
                        "5.000000",
                        available_quote("5.000000", "2026-08-19T10:01:00Z"),
                    )],
                )),
                CommandResolution::Resolved(result(
                    &base_command,
                    evidence("base-mainnet", "2026-08-19T10:02:00Z"),
                    vec![resolved_item(
                        "base-mainnet",
                        "usdc",
                        "2.000000",
                        available_quote("2.000000", "2026-08-19T10:03:00Z"),
                    )],
                )),
            ],
        );

        assert_eq!(portfolio.resolved_at, SystemTime::UNIX_EPOCH);
        assert_eq!(portfolio.quote_currency, "USD");
        assert_eq!(
            portfolio.valuation_status,
            PortfolioObservationStatus::Complete
        );
        assert_eq!(portfolio.known_value, "7.000000");
        assert_eq!(
            portfolio
                .members
                .iter()
                .map(|observation| observation.member.public_id.as_str())
                .collect::<Vec<_>>(),
            vec!["wma_base", "wma_eth"]
        );
        assert_eq!(portfolio.assets.len(), 1);
        let asset = &portfolio.assets[0];
        assert_eq!(asset.asset_slug, "usdc");
        assert_eq!(asset.total_amount.as_deref(), Some("7.000000"));
        assert_eq!(asset.known_value, "7.000000");
        assert_eq!(asset.contributions.len(), 2);
        assert_eq!(asset.contributions[0].network_slug, "base-mainnet");
        assert_eq!(asset.contributions[1].network_slug, "eth-mainnet");
        assert_eq!(asset.contributions[1].labels, vec!["Treasury"]);
        assert!(matches!(
            &asset.contributions[0].outcome,
            PortfolioContributionOutcome::Balance(BalanceItemOutcome::Resolved {
                quote: BalanceQuoteOutcome::Available { price_as_of, .. },
                ..
            }) if price_as_of == "2026-08-19T10:03:00Z"
        ));
        assert_eq!(
            asset.contributions[1]
                .evidence
                .as_ref()
                .unwrap()
                .observed_at,
            "2026-08-19T10:00:00Z"
        );
    }

    #[test]
    fn keeps_unpriced_zero_balance_visible_and_marks_portfolio_partial() {
        let member = member(
            "eth-mainnet",
            "0x1111111111111111111111111111111111111111",
            "wma_eth",
        );
        let command = command("eth-mainnet", &member.address, &["usdc", "ethereum"]);
        let portfolio = compose(
            vec![member],
            &[command.clone()],
            vec![CommandResolution::Resolved(result(
                &command,
                evidence("eth-mainnet", "2026-08-19T10:00:00Z"),
                vec![
                    resolved_item(
                        "eth-mainnet",
                        "usdc",
                        "0.000000",
                        BalanceQuoteOutcome::Unavailable {
                            code: BalanceItemErrorCode::PriceResolutionFailed,
                        },
                    ),
                    resolved_item(
                        "eth-mainnet",
                        "ethereum",
                        "2.000000",
                        available_quote("2.000000", "2026-08-19T10:01:00Z"),
                    ),
                ],
            ))],
        );

        assert_eq!(
            portfolio.valuation_status,
            PortfolioObservationStatus::Partial
        );
        assert_eq!(portfolio.known_value, "2.000000");
        let usdc = portfolio
            .assets
            .iter()
            .find(|asset| asset.asset_slug == "usdc")
            .unwrap();
        assert_eq!(usdc.total_amount.as_deref(), Some("0.000000"));
        assert_eq!(usdc.known_value, "0");
        assert_eq!(usdc.valuation_status, PortfolioObservationStatus::Partial);
    }

    #[test]
    fn marks_a_non_empty_workspace_unavailable_when_every_command_fails() {
        let member = member(
            "eth-mainnet",
            "0x1111111111111111111111111111111111111111",
            "wma_eth",
        );
        let command = command("eth-mainnet", &member.address, &["usdc"]);
        let portfolio = compose(
            vec![member],
            &[command],
            vec![CommandResolution::Unavailable],
        );

        assert_eq!(
            portfolio.valuation_status,
            PortfolioObservationStatus::Unavailable
        );
        assert_eq!(portfolio.known_value, "0");
        assert!(portfolio.assets.is_empty());
        assert!(matches!(
            portfolio.members[0].contributions[0].outcome,
            PortfolioContributionOutcome::CommandUnavailable { ref asset_slug }
                if asset_slug == "usdc"
        ));
    }

    #[test]
    fn preserves_resolved_assets_when_another_expected_balance_fails() {
        let member = member(
            "eth-mainnet",
            "0x1111111111111111111111111111111111111111",
            "wma_eth",
        );
        let command = command("eth-mainnet", &member.address, &["usdc", "ethereum"]);
        let portfolio = compose(
            vec![member],
            &[command.clone()],
            vec![CommandResolution::Resolved(result(
                &command,
                evidence("eth-mainnet", "2026-08-19T10:00:00Z"),
                vec![
                    resolved_item(
                        "eth-mainnet",
                        "usdc",
                        "1.000000",
                        available_quote("1.000000", "2026-08-19T10:01:00Z"),
                    ),
                    BalanceItemOutcome::Failed {
                        target: target("eth-mainnet", "ethereum"),
                        code: BalanceItemErrorCode::BalanceProviderUnavailable,
                    },
                ],
            ))],
        );

        assert_eq!(
            portfolio.valuation_status,
            PortfolioObservationStatus::Partial
        );
        assert_eq!(portfolio.known_value, "1.000000");
        assert_eq!(portfolio.assets.len(), 1);
        assert_eq!(portfolio.assets[0].asset_slug, "usdc");
        assert_eq!(
            portfolio.members[0].observation_status,
            PortfolioObservationStatus::Partial
        );
    }

    #[test]
    fn empty_workspace_is_a_complete_zero_value_observation() {
        let portfolio = compose(Vec::new(), &[], Vec::new());

        assert_eq!(
            portfolio.valuation_status,
            PortfolioObservationStatus::Complete
        );
        assert_eq!(portfolio.known_value, "0");
        assert!(portfolio.members.is_empty());
        assert!(portfolio.assets.is_empty());
    }

    #[tokio::test]
    async fn resolver_returns_an_unavailable_observation_when_the_balance_provider_is_disabled() {
        let registry = embedded_canonical_registry();
        let resolver = WorkspacePortfolioResolver::new(
            registry.clone(),
            BalanceSnapshotService::new(
                CatalogBalanceTargetResolver::new(registry),
                None,
                None::<NoopQuoteReader>,
            ),
        );
        let member = member(
            "eth-mainnet",
            "0x1111111111111111111111111111111111111111",
            "wma_eth",
        );
        let before = SystemTime::now();
        let portfolio = resolver.resolve(workspace(), vec![member]).await.unwrap();

        assert!(portfolio.resolved_at >= before);
        assert_eq!(
            portfolio.valuation_status,
            PortfolioObservationStatus::Unavailable
        );
        assert_eq!(portfolio.known_value, "0");
        assert!(portfolio.members[0]
            .contributions
            .iter()
            .all(|contribution| matches!(
                contribution.outcome,
                PortfolioContributionOutcome::Balance(BalanceItemOutcome::Failed { .. })
            )));
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
