use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::application::balances::service::{
    BalanceAccountResult, BalanceAsOf, BalanceEvidence, BalanceItemErrorCode, BalanceItemOutcome,
    BalanceQuoteOutcome, BalanceSnapshotResult, BalanceTokenSelector, ResolvedBalanceTarget,
};
use crate::domain::accounts::OnchainAccount;

#[allow(dead_code)]
pub(crate) mod examples;
pub(crate) mod requests;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BalanceResponseAssembler;

impl BalanceResponseAssembler {
    pub(crate) fn single(
        &self,
        snapshot: BalanceSnapshotResult,
    ) -> Result<SingleBalanceResponse, BalanceResponseAssemblerError> {
        let mut accounts = snapshot.accounts;
        if accounts.len() != 1 {
            return Err(BalanceResponseAssemblerError::ExpectedSingleAccount);
        }

        let account = shape_account(accounts.pop().expect("single account length checked"));
        let as_of = shape_as_of(&snapshot.as_of, account.evidence.as_ref());
        Ok(SingleBalanceResponse {
            ok: true,
            response_type: "balances".to_string(),
            status: account.status,
            as_of,
            quote_currency: snapshot.quote_currency,
            account: account.account,
            evidence: account.evidence,
            positions: account.positions,
            skipped: account.skipped,
            errors: account.errors,
        })
    }

    pub(crate) fn bulk(&self, snapshot: BalanceSnapshotResult) -> BulkBalanceResponse {
        let requested_accounts = snapshot.accounts.len();
        let requested_assets = snapshot.requested_token_count;
        let accounts = snapshot
            .accounts
            .into_iter()
            .map(shape_account)
            .collect::<Vec<_>>();
        let positions_returned = accounts.iter().map(|account| account.positions.len()).sum();
        let skipped_items = accounts.iter().map(|account| account.skipped.len()).sum();
        let failed_items = accounts
            .iter()
            .map(|account| account.failed_balance_items)
            .sum();
        let status = aggregate_bulk_status(&accounts);

        let as_of = shape_as_of(&snapshot.as_of, None);
        BulkBalanceResponse {
            ok: true,
            response_type: "balances_bulk".to_string(),
            status,
            as_of,
            quote_currency: snapshot.quote_currency,
            summary: BalanceSummaryPayload {
                requested_accounts,
                requested_assets,
                requested_resolution_items: requested_accounts.saturating_mul(requested_assets),
                positions_returned,
                skipped_items,
                failed_items,
            },
            accounts: accounts
                .into_iter()
                .map(|account| BalanceAccountPayload {
                    status: account.status,
                    account: account.account,
                    evidence: account.evidence,
                    positions: account.positions,
                    skipped: account.skipped,
                    errors: account.errors,
                })
                .collect(),
            errors: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BalanceResponseAssemblerError {
    ExpectedSingleAccount,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum BalanceResponseStatus {
    Complete,
    Partial,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct SingleBalanceResponse {
    ok: bool,
    #[serde(rename = "type")]
    response_type: String,
    status: BalanceResponseStatus,
    as_of: BalanceAsOfPayload,
    quote_currency: String,
    account: BalanceAccountIdentityPayload,
    evidence: Option<BalanceEvidencePayload>,
    positions: Vec<BalancePositionPayload>,
    skipped: Vec<BalanceSkippedPayload>,
    errors: Vec<BalanceErrorPayload>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct BulkBalanceResponse {
    ok: bool,
    #[serde(rename = "type")]
    response_type: String,
    status: BalanceResponseStatus,
    as_of: BalanceAsOfPayload,
    quote_currency: String,
    summary: BalanceSummaryPayload,
    accounts: Vec<BalanceAccountPayload>,
    errors: Vec<BalanceErrorPayload>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalanceAsOfPayload {
    kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timestamp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    block_number: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    observed_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalanceSummaryPayload {
    requested_accounts: usize,
    requested_assets: usize,
    requested_resolution_items: usize,
    positions_returned: usize,
    skipped_items: usize,
    failed_items: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalanceAccountPayload {
    status: BalanceResponseStatus,
    account: BalanceAccountIdentityPayload,
    evidence: Option<BalanceEvidencePayload>,
    positions: Vec<BalancePositionPayload>,
    skipped: Vec<BalanceSkippedPayload>,
    errors: Vec<BalanceErrorPayload>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalanceAccountIdentityPayload {
    network_slug: String,
    address: String,
    client_ref: Option<String>,
}

impl From<OnchainAccount> for BalanceAccountIdentityPayload {
    fn from(account: OnchainAccount) -> Self {
        Self {
            network_slug: account.network_slug,
            address: account.address,
            client_ref: account.client_ref,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalanceEvidencePayload {
    source: String,
    network_slug: String,
    block: BalanceBlockPayload,
    observed_at: String,
}

impl From<BalanceEvidence> for BalanceEvidencePayload {
    fn from(evidence: BalanceEvidence) -> Self {
        Self {
            source: "bigwig".to_string(),
            network_slug: evidence.network_slug,
            block: BalanceBlockPayload {
                number: evidence.block_number,
                hash: evidence.block_hash,
                timestamp: evidence.block_timestamp,
            },
            observed_at: evidence.observed_at,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalanceBlockPayload {
    number: String,
    hash: String,
    timestamp: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalancePositionPayload {
    selector: BalanceSelectorPayload,
    network_slug: String,
    contract_address: Option<String>,
    asset_slug: Option<String>,
    symbol: Option<String>,
    balance: BalanceAmountPayload,
    quote: BalanceQuotePayload,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalanceSelectorPayload {
    kind: String,
    value: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalanceAmountPayload {
    raw_amount: String,
    amount: Option<String>,
    decimals: Option<u8>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalanceQuotePayload {
    status: BalanceQuoteStatus,
    currency: Option<String>,
    unit_price: Option<String>,
    value: Option<String>,
    price_as_of: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum BalanceQuoteStatus {
    Available,
    Unavailable,
    Unsupported,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalanceSkippedPayload {
    network_slug: String,
    asset_slug: String,
    reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalanceErrorPayload {
    network_slug: String,
    selector: BalanceSelectorPayload,
    contract_address: Option<String>,
    asset_slug: Option<String>,
    code: String,
    message: String,
}

struct ShapedAccount {
    status: BalanceResponseStatus,
    account: BalanceAccountIdentityPayload,
    evidence: Option<BalanceEvidencePayload>,
    positions: Vec<BalancePositionPayload>,
    skipped: Vec<BalanceSkippedPayload>,
    errors: Vec<BalanceErrorPayload>,
    supported_balance_items: usize,
    resolved_balance_items: usize,
    failed_balance_items: usize,
}

fn shape_account(account: BalanceAccountResult) -> ShapedAccount {
    let mut positions = Vec::new();
    let mut skipped = Vec::new();
    let mut errors = Vec::new();
    let mut supported_balance_items = 0usize;
    let mut resolved_balance_items = 0usize;
    let mut failed_balance_items = 0usize;
    let mut degraded_quote = false;

    for item in account.items {
        match item {
            BalanceItemOutcome::Resolved {
                target,
                raw_amount,
                amount,
                quote,
            } => {
                supported_balance_items += 1;
                resolved_balance_items += 1;
                let (quote, error) = shape_quote(&target, quote);
                degraded_quote |= quote.status != BalanceQuoteStatus::Available;
                if let Some(error) = error {
                    errors.push(error);
                }
                let selector = selector_payload(&target.selector);
                let contract_address = contract_address(&target);
                positions.push(BalancePositionPayload {
                    selector,
                    network_slug: target.network_slug,
                    contract_address,
                    asset_slug: target.asset_slug,
                    symbol: target.symbol,
                    balance: BalanceAmountPayload {
                        raw_amount,
                        amount,
                        decimals: target.decimals,
                    },
                    quote,
                });
            }
            BalanceItemOutcome::Skipped {
                network_slug,
                asset_slug,
            } => skipped.push(BalanceSkippedPayload {
                network_slug,
                asset_slug,
                reason: "asset_not_supported_on_network".to_string(),
            }),
            BalanceItemOutcome::Failed { target, code } => {
                supported_balance_items += 1;
                failed_balance_items += 1;
                errors.push(error_payload(&target, code));
            }
        }
    }

    let status = account_status(
        supported_balance_items,
        resolved_balance_items,
        failed_balance_items,
        degraded_quote,
    );

    ShapedAccount {
        status,
        account: account.account.into(),
        evidence: account.evidence.map(BalanceEvidencePayload::from),
        positions,
        skipped,
        errors,
        supported_balance_items,
        resolved_balance_items,
        failed_balance_items,
    }
}

fn shape_as_of(
    as_of: &BalanceAsOf,
    evidence: Option<&BalanceEvidencePayload>,
) -> BalanceAsOfPayload {
    match as_of {
        BalanceAsOf::Latest => BalanceAsOfPayload {
            kind: "latest".to_string(),
            timestamp: None,
            block_number: None,
            observed_at: evidence.map(|evidence| evidence.observed_at.clone()),
        },
        BalanceAsOf::Timestamp { timestamp } => BalanceAsOfPayload {
            kind: "timestamp".to_string(),
            timestamp: Some(timestamp.clone()),
            block_number: None,
            observed_at: None,
        },
        BalanceAsOf::BlockNumber { block_number } => BalanceAsOfPayload {
            kind: "block_number".to_string(),
            timestamp: None,
            block_number: Some(block_number.clone()),
            observed_at: None,
        },
    }
}

fn shape_quote(
    target: &ResolvedBalanceTarget,
    quote: BalanceQuoteOutcome,
) -> (BalanceQuotePayload, Option<BalanceErrorPayload>) {
    match quote {
        BalanceQuoteOutcome::Available {
            currency,
            unit_price,
            value,
            price_as_of,
        } => (
            BalanceQuotePayload {
                status: BalanceQuoteStatus::Available,
                currency: Some(currency),
                unit_price: Some(unit_price),
                value: Some(value),
                price_as_of: Some(price_as_of),
            },
            None,
        ),
        BalanceQuoteOutcome::Unavailable { code } => (
            BalanceQuotePayload {
                status: BalanceQuoteStatus::Unavailable,
                currency: None,
                unit_price: None,
                value: None,
                price_as_of: None,
            },
            Some(error_payload(target, code)),
        ),
        BalanceQuoteOutcome::Unsupported => (
            BalanceQuotePayload {
                status: BalanceQuoteStatus::Unsupported,
                currency: None,
                unit_price: None,
                value: None,
                price_as_of: None,
            },
            None,
        ),
    }
}

fn error_payload(
    target: &ResolvedBalanceTarget,
    code: BalanceItemErrorCode,
) -> BalanceErrorPayload {
    let (code, message) = match code {
        BalanceItemErrorCode::BalanceResolutionFailed => (
            "balance_resolution_failed",
            "Balance could not be resolved for this asset on this network.",
        ),
        BalanceItemErrorCode::BalanceProviderUnavailable => (
            "balance_provider_unavailable",
            "Balance is temporarily unavailable for this asset on this network.",
        ),
        BalanceItemErrorCode::PriceResolutionFailed => (
            "price_resolution_failed",
            "Quote could not be resolved for this asset.",
        ),
        BalanceItemErrorCode::PriceProviderUnavailable => (
            "price_provider_unavailable",
            "Quote is temporarily unavailable for this asset.",
        ),
        BalanceItemErrorCode::InternalError => (
            "internal_error",
            "This balance item could not be processed.",
        ),
    };

    BalanceErrorPayload {
        network_slug: target.network_slug.clone(),
        selector: selector_payload(&target.selector),
        contract_address: contract_address(target),
        asset_slug: target.asset_slug.clone(),
        code: code.to_string(),
        message: message.to_string(),
    }
}

fn selector_payload(selector: &BalanceTokenSelector) -> BalanceSelectorPayload {
    match selector {
        BalanceTokenSelector::AssetSlug(asset_slug) => BalanceSelectorPayload {
            kind: "asset_slug".to_string(),
            value: asset_slug.clone(),
        },
        BalanceTokenSelector::ContractAddress(contract_address) => BalanceSelectorPayload {
            kind: "contract_address".to_string(),
            value: contract_address.clone(),
        },
    }
}

fn contract_address(target: &ResolvedBalanceTarget) -> Option<String> {
    match &target.kind {
        crate::domain::assets::balance_catalog::BalanceTargetKind::Native => None,
        crate::domain::assets::balance_catalog::BalanceTargetKind::Erc20 { contract_address } => {
            Some(contract_address.clone())
        }
    }
}

fn account_status(
    supported_balance_items: usize,
    resolved_balance_items: usize,
    failed_balance_items: usize,
    degraded_quote: bool,
) -> BalanceResponseStatus {
    if supported_balance_items == 0 {
        BalanceResponseStatus::Complete
    } else if resolved_balance_items == 0 {
        BalanceResponseStatus::Failed
    } else if failed_balance_items > 0 || degraded_quote {
        BalanceResponseStatus::Partial
    } else {
        BalanceResponseStatus::Complete
    }
}

fn aggregate_bulk_status(accounts: &[ShapedAccount]) -> BalanceResponseStatus {
    let supported_balance_items = accounts
        .iter()
        .map(|account| account.supported_balance_items)
        .sum::<usize>();
    let resolved_balance_items = accounts
        .iter()
        .map(|account| account.resolved_balance_items)
        .sum::<usize>();

    if supported_balance_items == 0 {
        BalanceResponseStatus::Complete
    } else if resolved_balance_items == 0 {
        BalanceResponseStatus::Failed
    } else if accounts
        .iter()
        .any(|account| account.status != BalanceResponseStatus::Complete)
    {
        BalanceResponseStatus::Partial
    } else {
        BalanceResponseStatus::Complete
    }
}

#[cfg(test)]
mod tests;
