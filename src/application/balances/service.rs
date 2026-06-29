use std::{
    collections::{HashMap, HashSet},
    fmt,
};

use tokio::task::JoinSet;
use tracing::warn;

use crate::adapters::bigwig::balances::{
    BigwigAccount, BigwigEvidenceItem, BigwigEvidenceStatus, BigwigPrimitive, BigwigRequest,
    BigwigResponse, BigwigTarget,
};
use crate::adapters::bigwig::client::BigwigClient;
use crate::adapters::bigwig::error::BigwigError;
use crate::domain::balance_catalog::{BalanceTarget, BalanceTargetKind, CatalogResolverError};

use super::{
    catalog::{BalanceTargetResolution, CatalogBalanceTargetResolver},
    decimal::{format_amount, is_unsigned_integer, multiply_amount_by_price},
    quote::{PriceQuoteClient, PriceQuoteClientError, PriceQuoteResolution},
};

const BIGWIG_MAX_ACCOUNTS: usize = 50;
const BIGWIG_MAX_TARGETS: usize = 20;
const BIGWIG_MAX_ITEMS: usize = 1_000;

#[derive(Clone, Debug)]
pub struct BalanceSnapshotService {
    catalog_resolver: CatalogBalanceTargetResolver,
    bigwig_client: Option<BigwigClient>,
    price_quote_client: Option<PriceQuoteClient>,
}

impl BalanceSnapshotService {
    pub fn new(
        catalog_resolver: CatalogBalanceTargetResolver,
        bigwig_client: Option<BigwigClient>,
        price_quote_client: Option<PriceQuoteClient>,
    ) -> Self {
        Self {
            catalog_resolver,
            bigwig_client,
            price_quote_client,
        }
    }

    pub async fn resolve_latest(
        &self,
        request: BalanceSnapshotRequest,
    ) -> Result<BalanceSnapshotResult, BalanceSnapshotServiceError> {
        let plans = self.plan_groups(&request).await?;
        let mut executions = (0..plans.len()).map(|_| None).collect::<Vec<_>>();
        let mut calls = JoinSet::new();

        for (group_index, plan) in plans.iter().enumerate() {
            if plan.targets.is_empty() {
                executions[group_index] = Some(GroupExecution::SkippedOnly);
                continue;
            }

            let Some(client) = self.bigwig_client.clone() else {
                executions[group_index] = Some(GroupExecution::Failed(
                    BalanceItemErrorCode::BalanceProviderUnavailable,
                ));
                continue;
            };

            let bigwig_request = plan.bigwig_request();
            calls
                .spawn(async move { (group_index, client.latest_balances(&bigwig_request).await) });
        }

        while let Some(joined) = calls.join_next().await {
            let (group_index, response) =
                joined.map_err(|_| BalanceSnapshotServiceError::ExecutionTaskFailed)?;
            executions[group_index] = Some(GroupExecution::Called(response));
        }

        let mut raw_account_results = (0..request.accounts.len())
            .map(|_| None)
            .collect::<Vec<_>>();

        for (group_index, plan) in plans.iter().enumerate() {
            let execution = executions[group_index]
                .take()
                .expect("every planned balance group must have an execution outcome");
            let group_results = assemble_group_results(plan, execution);

            for (account_index, result) in group_results {
                raw_account_results[account_index] = Some(result);
            }
        }

        let raw_account_results = raw_account_results
            .into_iter()
            .map(|result| result.expect("every requested account must belong to one group"))
            .collect::<Vec<_>>();
        let pricing_asset_slugs = collect_pricing_asset_slugs(&raw_account_results);
        let quotes = if pricing_asset_slugs.is_empty() {
            Ok(HashMap::new())
        } else {
            match &self.price_quote_client {
                Some(client) => {
                    client
                        .latest_quotes(&pricing_asset_slugs, &request.quote_currency)
                        .await
                }
                None => Err(PriceQuoteClientError::ProviderUnavailable),
            }
        };

        Ok(BalanceSnapshotResult {
            quote_currency: request.quote_currency,
            requested_asset_slugs: request.asset_slugs,
            accounts: enrich_account_results(raw_account_results, quotes),
        })
    }

    async fn plan_groups(
        &self,
        request: &BalanceSnapshotRequest,
    ) -> Result<Vec<NetworkGroupPlan>, BalanceSnapshotServiceError> {
        let grouped_accounts = group_accounts(&request.accounts);
        let mut plans = Vec::with_capacity(grouped_accounts.len());

        for group in grouped_accounts {
            let resolutions = self
                .catalog_resolver
                .resolve_network(&group.network_slug, &request.asset_slugs)
                .await?;
            plans.push(plan_network_group(
                group,
                &request.asset_slugs,
                resolutions,
            )?);
        }

        Ok(plans)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BalanceSnapshotRequest {
    pub accounts: Vec<BalanceSnapshotAccount>,
    pub asset_slugs: Vec<String>,
    pub quote_currency: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BalanceSnapshotAccount {
    pub network_slug: String,
    pub address: String,
    pub client_ref: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BalanceSnapshotResult {
    pub quote_currency: String,
    pub requested_asset_slugs: Vec<String>,
    pub accounts: Vec<BalanceAccountResult>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BalanceAccountResult {
    pub account: BalanceSnapshotAccount,
    pub evidence: Option<BalanceEvidence>,
    pub items: Vec<BalanceItemOutcome>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BalanceEvidence {
    pub network_slug: String,
    pub observed_at: String,
    pub block_number: String,
    pub block_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BalanceItemOutcome {
    Resolved {
        target: BalanceTarget,
        raw_amount: String,
        amount: String,
        quote: BalanceQuoteOutcome,
    },
    Skipped {
        network_slug: String,
        asset_slug: String,
    },
    Failed {
        target: BalanceTarget,
        code: BalanceItemErrorCode,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BalanceItemErrorCode {
    BalanceResolutionFailed,
    BalanceProviderUnavailable,
    PriceResolutionFailed,
    PriceProviderUnavailable,
    InternalError,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BalanceQuoteOutcome {
    Available {
        currency: String,
        unit_price: String,
        value: String,
        price_as_of: String,
    },
    Unavailable {
        code: BalanceItemErrorCode,
    },
    Unsupported,
}

#[derive(Clone, Debug)]
struct RawBalanceAccountResult {
    account: BalanceSnapshotAccount,
    evidence: Option<BalanceEvidence>,
    items: Vec<RawBalanceItemOutcome>,
}

#[derive(Clone, Debug)]
enum RawBalanceItemOutcome {
    Resolved {
        target: BalanceTarget,
        raw_amount: String,
    },
    Skipped {
        network_slug: String,
        asset_slug: String,
    },
    Failed {
        target: BalanceTarget,
        code: BalanceItemErrorCode,
    },
}

#[derive(Debug)]
pub enum BalanceSnapshotServiceError {
    Catalog(CatalogResolverError),
    UnsupportedNetwork {
        network_slug: String,
    },
    UnsupportedAsset {
        network_slug: String,
        asset_slug: String,
    },
    RequestTooLarge {
        network_slug: String,
    },
    InvalidPlan {
        network_slug: String,
        issue: BalancePlanIssue,
    },
    ExecutionTaskFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BalancePlanIssue {
    ResolutionCountMismatch,
    UnexpectedResolutionNetwork,
    InconsistentChainId,
    TargetCollision,
    ConflictingTargetMetadata,
}

impl From<CatalogResolverError> for BalanceSnapshotServiceError {
    fn from(error: CatalogResolverError) -> Self {
        Self::Catalog(error)
    }
}

impl fmt::Display for BalanceSnapshotServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "balance catalog resolution failed: {error}"),
            Self::UnsupportedNetwork { network_slug } => {
                write!(formatter, "unsupported balance network: {network_slug}")
            }
            Self::UnsupportedAsset {
                network_slug,
                asset_slug,
            } => write!(
                formatter,
                "unsupported balance asset {asset_slug} while planning network {network_slug}"
            ),
            Self::RequestTooLarge { network_slug } => {
                write!(
                    formatter,
                    "Bigwig balance group is too large: {network_slug}"
                )
            }
            Self::InvalidPlan {
                network_slug,
                issue,
            } => write!(
                formatter,
                "invalid balance orchestration plan for {network_slug}: {issue:?}"
            ),
            Self::ExecutionTaskFailed => write!(formatter, "balance orchestration task failed"),
        }
    }
}

impl std::error::Error for BalanceSnapshotServiceError {}

#[derive(Clone, Debug)]
struct GroupedAccounts {
    network_slug: String,
    accounts: Vec<GroupAccount>,
}

#[derive(Clone, Debug)]
struct GroupAccount {
    original_index: usize,
    account: BalanceSnapshotAccount,
}

#[derive(Clone, Debug)]
struct NetworkGroupPlan {
    network_slug: String,
    chain_id: Option<i64>,
    accounts: Vec<GroupAccount>,
    asset_plans: Vec<AssetPlan>,
    targets: Vec<PlannedTarget>,
}

impl NetworkGroupPlan {
    fn bigwig_request(&self) -> BigwigRequest {
        BigwigRequest {
            network_slug: self.network_slug.clone(),
            accounts: self
                .accounts
                .iter()
                .map(|account| BigwigAccount {
                    address: account.account.address.clone(),
                })
                .collect(),
            targets: self
                .targets
                .iter()
                .map(|target| target.wire_target.clone())
                .collect(),
        }
    }
}

#[derive(Clone, Debug)]
enum AssetPlan {
    Supported {
        target_index: usize,
        target: BalanceTarget,
    },
    Skipped {
        network_slug: String,
        asset_slug: String,
    },
}

#[derive(Clone, Debug)]
struct PlannedTarget {
    wire_target: BigwigTarget,
    catalog_target: BalanceTarget,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum TargetKey {
    Native,
    Erc20(String),
}

impl TargetKey {
    fn from_catalog(kind: &BalanceTargetKind) -> Self {
        match kind {
            BalanceTargetKind::Native => Self::Native,
            BalanceTargetKind::Erc20 { contract_address } => {
                Self::Erc20(contract_address.to_ascii_lowercase())
            }
        }
    }

    fn from_bigwig(target: &BigwigTarget) -> Self {
        match target {
            BigwigTarget::Native => Self::Native,
            BigwigTarget::Erc20 { contract_address } => {
                Self::Erc20(contract_address.to_ascii_lowercase())
            }
        }
    }
}

fn group_accounts(accounts: &[BalanceSnapshotAccount]) -> Vec<GroupedAccounts> {
    let mut group_indexes = HashMap::new();
    let mut groups = Vec::<GroupedAccounts>::new();

    for (original_index, account) in accounts.iter().cloned().enumerate() {
        let group_index = match group_indexes.get(&account.network_slug) {
            Some(index) => *index,
            None => {
                let index = groups.len();
                group_indexes.insert(account.network_slug.clone(), index);
                groups.push(GroupedAccounts {
                    network_slug: account.network_slug.clone(),
                    accounts: Vec::new(),
                });
                index
            }
        };

        groups[group_index].accounts.push(GroupAccount {
            original_index,
            account,
        });
    }

    groups
}

fn plan_network_group(
    group: GroupedAccounts,
    requested_asset_slugs: &[String],
    resolutions: Vec<BalanceTargetResolution>,
) -> Result<NetworkGroupPlan, BalanceSnapshotServiceError> {
    if resolutions.len() != requested_asset_slugs.len() {
        return Err(invalid_plan(
            &group.network_slug,
            BalancePlanIssue::ResolutionCountMismatch,
        ));
    }

    let mut chain_id = None;
    let mut targets = Vec::<PlannedTarget>::new();
    let mut target_indexes = HashMap::<TargetKey, usize>::new();
    let mut asset_plans = Vec::with_capacity(resolutions.len());

    for (requested_asset_slug, resolution) in requested_asset_slugs.iter().zip(resolutions) {
        match resolution {
            BalanceTargetResolution::UnsupportedNetwork { network_slug, .. } => {
                return Err(BalanceSnapshotServiceError::UnsupportedNetwork { network_slug });
            }
            BalanceTargetResolution::UnsupportedAsset {
                network_slug,
                asset_slug,
            } => {
                return Err(BalanceSnapshotServiceError::UnsupportedAsset {
                    network_slug,
                    asset_slug,
                });
            }
            BalanceTargetResolution::UnsupportedPair {
                network_slug,
                asset_slug,
            }
            | BalanceTargetResolution::UnsupportedTokenStandard {
                network_slug,
                asset_slug,
            } => {
                if network_slug != group.network_slug || asset_slug != *requested_asset_slug {
                    return Err(invalid_plan(
                        &group.network_slug,
                        BalancePlanIssue::UnexpectedResolutionNetwork,
                    ));
                }
                asset_plans.push(AssetPlan::Skipped {
                    network_slug,
                    asset_slug,
                });
            }
            BalanceTargetResolution::Resolved(target) => {
                if target.network_slug != group.network_slug
                    || target.asset_slug != *requested_asset_slug
                {
                    return Err(invalid_plan(
                        &group.network_slug,
                        BalancePlanIssue::UnexpectedResolutionNetwork,
                    ));
                }

                match chain_id {
                    Some(expected_chain_id) if expected_chain_id != target.chain_id => {
                        return Err(invalid_plan(
                            &group.network_slug,
                            BalancePlanIssue::InconsistentChainId,
                        ));
                    }
                    None => chain_id = Some(target.chain_id),
                    _ => {}
                }

                let key = TargetKey::from_catalog(&target.kind);
                let target_index = if let Some(existing_index) = target_indexes.get(&key).copied() {
                    let existing = &targets[existing_index].catalog_target;
                    if existing.asset_slug != target.asset_slug {
                        return Err(invalid_plan(
                            &group.network_slug,
                            BalancePlanIssue::TargetCollision,
                        ));
                    }
                    if existing != &target {
                        return Err(invalid_plan(
                            &group.network_slug,
                            BalancePlanIssue::ConflictingTargetMetadata,
                        ));
                    }
                    existing_index
                } else {
                    let target_index = targets.len();
                    target_indexes.insert(key, target_index);
                    targets.push(PlannedTarget {
                        wire_target: bigwig_target(&target.kind),
                        catalog_target: target.clone(),
                    });
                    target_index
                };

                asset_plans.push(AssetPlan::Supported {
                    target_index,
                    target,
                });
            }
        }
    }

    let item_count = group.accounts.len().saturating_mul(targets.len());
    if group.accounts.len() > BIGWIG_MAX_ACCOUNTS
        || targets.len() > BIGWIG_MAX_TARGETS
        || item_count > BIGWIG_MAX_ITEMS
    {
        return Err(BalanceSnapshotServiceError::RequestTooLarge {
            network_slug: group.network_slug,
        });
    }

    Ok(NetworkGroupPlan {
        network_slug: group.network_slug,
        chain_id,
        accounts: group.accounts,
        asset_plans,
        targets,
    })
}

fn invalid_plan(network_slug: &str, issue: BalancePlanIssue) -> BalanceSnapshotServiceError {
    BalanceSnapshotServiceError::InvalidPlan {
        network_slug: network_slug.to_string(),
        issue,
    }
}

fn bigwig_target(kind: &BalanceTargetKind) -> BigwigTarget {
    match kind {
        BalanceTargetKind::Native => BigwigTarget::Native,
        BalanceTargetKind::Erc20 { contract_address } => BigwigTarget::Erc20 {
            contract_address: contract_address.clone(),
        },
    }
}

enum GroupExecution {
    SkippedOnly,
    Failed(BalanceItemErrorCode),
    Called(Result<BigwigResponse, BigwigError>),
}

#[derive(Clone, Debug)]
struct ValidatedGroup {
    evidence: BalanceEvidence,
    target_outcomes: Vec<TargetOutcome>,
}

#[derive(Clone, Debug)]
enum TargetOutcome {
    Resolved(String),
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResponseValidationIssue {
    WrongPrimitive,
    WrongNetwork,
    WrongChainId,
    WrongCardinality,
    UnexpectedCorrelation,
    DuplicateCorrelation,
    InvalidRawAmount,
    WrongStatus,
}

fn assemble_group_results(
    plan: &NetworkGroupPlan,
    execution: GroupExecution,
) -> Vec<(usize, RawBalanceAccountResult)> {
    let validated = match execution {
        GroupExecution::SkippedOnly => None,
        GroupExecution::Failed(code) => {
            return failed_group_results(plan, code);
        }
        GroupExecution::Called(Err(error)) => {
            log_bigwig_group_error(&plan.network_slug, &error);
            return failed_group_results(plan, map_bigwig_error(&error));
        }
        GroupExecution::Called(Ok(response)) => match validate_response(plan, response) {
            Ok(validated) => Some(validated),
            Err(issue) => {
                warn!(
                    network_slug = plan.network_slug,
                    ?issue,
                    "Bigwig latest-balance response failed orchestration validation"
                );
                return failed_group_results(plan, BalanceItemErrorCode::InternalError);
            }
        },
    };

    plan.accounts
        .iter()
        .enumerate()
        .map(|(group_account_index, group_account)| {
            let evidence = validated
                .as_ref()
                .map(|validated| validated.evidence.clone());
            let items = plan
                .asset_plans
                .iter()
                .map(|asset_plan| match asset_plan {
                    AssetPlan::Skipped {
                        network_slug,
                        asset_slug,
                    } => RawBalanceItemOutcome::Skipped {
                        network_slug: network_slug.clone(),
                        asset_slug: asset_slug.clone(),
                    },
                    AssetPlan::Supported {
                        target_index,
                        target,
                    } => {
                        let outcome_index = group_account_index * plan.targets.len() + target_index;
                        match &validated
                            .as_ref()
                            .expect("supported targets require a validated Bigwig response")
                            .target_outcomes[outcome_index]
                        {
                            TargetOutcome::Resolved(raw_amount) => {
                                RawBalanceItemOutcome::Resolved {
                                    target: target.clone(),
                                    raw_amount: raw_amount.clone(),
                                }
                            }
                            TargetOutcome::Failed => RawBalanceItemOutcome::Failed {
                                target: target.clone(),
                                code: BalanceItemErrorCode::BalanceResolutionFailed,
                            },
                        }
                    }
                })
                .collect();

            (
                group_account.original_index,
                RawBalanceAccountResult {
                    account: group_account.account.clone(),
                    evidence,
                    items,
                },
            )
        })
        .collect()
}

fn failed_group_results(
    plan: &NetworkGroupPlan,
    code: BalanceItemErrorCode,
) -> Vec<(usize, RawBalanceAccountResult)> {
    plan.accounts
        .iter()
        .map(|group_account| {
            let items = plan
                .asset_plans
                .iter()
                .map(|asset_plan| match asset_plan {
                    AssetPlan::Supported { target, .. } => RawBalanceItemOutcome::Failed {
                        target: target.clone(),
                        code,
                    },
                    AssetPlan::Skipped {
                        network_slug,
                        asset_slug,
                    } => RawBalanceItemOutcome::Skipped {
                        network_slug: network_slug.clone(),
                        asset_slug: asset_slug.clone(),
                    },
                })
                .collect();

            (
                group_account.original_index,
                RawBalanceAccountResult {
                    account: group_account.account.clone(),
                    evidence: None,
                    items,
                },
            )
        })
        .collect()
}

fn validate_response(
    plan: &NetworkGroupPlan,
    response: BigwigResponse,
) -> Result<ValidatedGroup, ResponseValidationIssue> {
    if response.primitive != BigwigPrimitive::EvmLatestBalances {
        return Err(ResponseValidationIssue::WrongPrimitive);
    }
    if response.network.network_slug != plan.network_slug {
        return Err(ResponseValidationIssue::WrongNetwork);
    }
    if Some(response.network.chain_id) != plan.chain_id.and_then(|value| u64::try_from(value).ok())
    {
        return Err(ResponseValidationIssue::WrongChainId);
    }

    let expected_item_count = plan.accounts.len() * plan.targets.len();
    if response.items.len() != expected_item_count {
        return Err(ResponseValidationIssue::WrongCardinality);
    }

    let mut correlations = HashSet::with_capacity(response.items.len());
    let mut target_outcomes = Vec::with_capacity(response.items.len());
    let mut resolved_count = 0usize;
    let mut failed_count = 0usize;

    for (item_index, item) in response.items.into_iter().enumerate() {
        let expected_account = &plan.accounts[item_index / plan.targets.len()];
        let expected_target = &plan.targets[item_index % plan.targets.len()];
        let (account, target) = match &item {
            BigwigEvidenceItem::Resolved {
                account, target, ..
            }
            | BigwigEvidenceItem::Failed {
                account, target, ..
            } => (account, target),
        };

        let account_address = account.address.clone();
        let normalized_account = account_address.to_ascii_lowercase();
        let target_key = TargetKey::from_bigwig(target);
        if !correlations.insert((normalized_account.clone(), target_key.clone())) {
            return Err(ResponseValidationIssue::DuplicateCorrelation);
        }
        if normalized_account != expected_account.account.address.to_ascii_lowercase()
            || target_key != TargetKey::from_bigwig(&expected_target.wire_target)
        {
            return Err(ResponseValidationIssue::UnexpectedCorrelation);
        }

        match item {
            BigwigEvidenceItem::Resolved { raw_amount, .. } => {
                if !is_unsigned_integer(&raw_amount) {
                    return Err(ResponseValidationIssue::InvalidRawAmount);
                }
                resolved_count += 1;
                target_outcomes.push(TargetOutcome::Resolved(raw_amount));
            }
            BigwigEvidenceItem::Failed { error, .. } => {
                failed_count += 1;
                warn!(
                    network_slug = plan.network_slug,
                    account_address,
                    provider_code = ?error.code,
                    "Bigwig latest-balance item failed"
                );
                target_outcomes.push(TargetOutcome::Failed);
            }
        }
    }

    let expected_status = match (resolved_count, failed_count) {
        (_, 0) => BigwigEvidenceStatus::Complete,
        (0, _) => BigwigEvidenceStatus::Failed,
        _ => BigwigEvidenceStatus::Partial,
    };
    if response.status != expected_status {
        return Err(ResponseValidationIssue::WrongStatus);
    }

    Ok(ValidatedGroup {
        evidence: BalanceEvidence {
            network_slug: response.network.network_slug,
            observed_at: response.observed_at,
            block_number: response.block.number,
            block_hash: response.block.hash,
        },
        target_outcomes,
    })
}

fn collect_pricing_asset_slugs(accounts: &[RawBalanceAccountResult]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut pricing_asset_slugs = Vec::new();

    for account in accounts {
        for item in &account.items {
            if let RawBalanceItemOutcome::Resolved { target, .. } = item {
                let pricing_asset_slug = normalize_pricing_asset_slug(&target.pricing_asset_slug);
                if seen.insert(pricing_asset_slug.clone()) {
                    pricing_asset_slugs.push(pricing_asset_slug);
                }
            }
        }
    }

    pricing_asset_slugs
}

fn normalize_pricing_asset_slug(pricing_asset_slug: &str) -> String {
    pricing_asset_slug.trim().to_ascii_lowercase()
}

fn enrich_account_results(
    accounts: Vec<RawBalanceAccountResult>,
    quotes: Result<HashMap<String, PriceQuoteResolution>, PriceQuoteClientError>,
) -> Vec<BalanceAccountResult> {
    accounts
        .into_iter()
        .map(|account| BalanceAccountResult {
            account: account.account,
            evidence: account.evidence,
            items: account
                .items
                .into_iter()
                .map(|item| enrich_item(item, &quotes))
                .collect(),
        })
        .collect()
}

fn enrich_item(
    item: RawBalanceItemOutcome,
    quotes: &Result<HashMap<String, PriceQuoteResolution>, PriceQuoteClientError>,
) -> BalanceItemOutcome {
    match item {
        RawBalanceItemOutcome::Resolved { target, raw_amount } => {
            let amount = format_amount(&raw_amount, target.decimals)
                .expect("validated Bigwig raw amount must format exactly");
            let quote = match quotes {
                Ok(quotes) => quotes
                    .get(&normalize_pricing_asset_slug(&target.pricing_asset_slug))
                    .map(|quote| enrich_quote(quote, &raw_amount, target.decimals))
                    .unwrap_or(BalanceQuoteOutcome::Unavailable {
                        code: BalanceItemErrorCode::InternalError,
                    }),
                Err(PriceQuoteClientError::ProviderUnavailable) => {
                    BalanceQuoteOutcome::Unavailable {
                        code: BalanceItemErrorCode::PriceProviderUnavailable,
                    }
                }
                Err(PriceQuoteClientError::InternalError) => BalanceQuoteOutcome::Unavailable {
                    code: BalanceItemErrorCode::InternalError,
                },
            };

            BalanceItemOutcome::Resolved {
                target,
                raw_amount,
                amount,
                quote,
            }
        }
        RawBalanceItemOutcome::Skipped {
            network_slug,
            asset_slug,
        } => BalanceItemOutcome::Skipped {
            network_slug,
            asset_slug,
        },
        RawBalanceItemOutcome::Failed { target, code } => {
            BalanceItemOutcome::Failed { target, code }
        }
    }
}

fn enrich_quote(
    quote: &PriceQuoteResolution,
    raw_amount: &str,
    decimals: u8,
) -> BalanceQuoteOutcome {
    match quote {
        PriceQuoteResolution::Available {
            unit_price,
            quote_currency,
            price_as_of,
        } => match multiply_amount_by_price(raw_amount, decimals, unit_price) {
            Ok(value) => BalanceQuoteOutcome::Available {
                currency: quote_currency.clone(),
                unit_price: unit_price.clone(),
                value,
                price_as_of: price_as_of.clone(),
            },
            Err(_) => BalanceQuoteOutcome::Unavailable {
                code: BalanceItemErrorCode::InternalError,
            },
        },
        PriceQuoteResolution::Unavailable => BalanceQuoteOutcome::Unavailable {
            code: BalanceItemErrorCode::PriceResolutionFailed,
        },
        PriceQuoteResolution::Unsupported => BalanceQuoteOutcome::Unsupported,
    }
}

fn map_bigwig_error(error: &BigwigError) -> BalanceItemErrorCode {
    match error {
        BigwigError::UnsupportedNetwork
        | BigwigError::NetworkNotEnabledForOperation
        | BigwigError::NoRouteSatisfiesOperation
        | BigwigError::RpcError => BalanceItemErrorCode::BalanceResolutionFailed,
        BigwigError::Transport
        | BigwigError::Timeout
        | BigwigError::Unauthorized
        | BigwigError::RateLimited { .. }
        | BigwigError::ProviderUnavailable { .. }
        | BigwigError::ExtractionTimeout
        | BigwigError::ProviderTimeout
        | BigwigError::InternalError => BalanceItemErrorCode::BalanceProviderUnavailable,
        BigwigError::InvalidExtractionRequest
        | BigwigError::InvalidAddress
        | BigwigError::InvalidContractAddress
        | BigwigError::InvalidDirection
        | BigwigError::InvalidWindowShape
        | BigwigError::ReversedBlockRange
        | BigwigError::BlockOutOfRange
        | BigwigError::ReversedTimestampRange
        | BigwigError::TimestampOutOfRange
        | BigwigError::LookbackTooLarge
        | BigwigError::RangeTooLarge
        | BigwigError::TooManyContractAddresses
        | BigwigError::RequestValidation(_)
        | BigwigError::MalformedSuccessResponse
        | BigwigError::MalformedErrorResponse
        | BigwigError::UnexpectedSuccessStatus(_)
        | BigwigError::UnexpectedErrorResponse { .. } => BalanceItemErrorCode::InternalError,
    }
}

fn log_bigwig_group_error(network_slug: &str, error: &BigwigError) {
    match error {
        BigwigError::RateLimited {
            retry_after_seconds,
        }
        | BigwigError::ProviderUnavailable {
            retry_after_seconds,
        } => warn!(
            network_slug,
            ?error,
            ?retry_after_seconds,
            "Bigwig latest-balance group failed"
        ),
        _ => warn!(network_slug, ?error, "Bigwig latest-balance group failed"),
    }
}

#[cfg(test)]
mod tests;

// mod tests {
//     use std::{
//         io::{Read, Write},
//         net::TcpListener,
//         thread,
//     };

//     use reqwest::StatusCode;
//     use serde_json::{json, Value};

//     use super::*;
//     use crate::test_utils::fixtures::global_assets::sample_assets;
//     use crate::{
//         adapters::bigwig::balances::{
//             BigwigEvidenceBlock, BigwigEvidenceNetwork, BigwigItemError, BigwigItemErrorCode,
//             BigwigRequestValidationCode,
//         },
//         adapters::postgres::global_assets::GlobalAssetRepository,
//         adapters::price_indexer::PriceIndexerClient,
//     };

//     const ACCOUNT_A: &str = "0x1111111111111111111111111111111111111111";
//     const ACCOUNT_B: &str = "0x2222222222222222222222222222222222222222";
//     const ACCOUNT_C: &str = "0x3333333333333333333333333333333333333333";
