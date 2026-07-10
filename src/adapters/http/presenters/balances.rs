use crate::{
    adapters::http::dto::balances::{
        BalanceAccountIdentityPayload, BalanceAccountPayload, BalanceAmountPayload,
        BalanceAsOfPayload, BalanceErrorPayload, BalanceEvidencePayload, BalancePositionPayload,
        BalanceQuotePayload, BalanceQuoteStatus, BalanceResponseStatus, BalanceSelectorPayload,
        BalanceSkippedPayload, BalanceSummaryPayload, BulkBalanceResponse, SingleBalanceResponse,
    },
    application::balances::{
        result::{BalancesAccountResult, GetBalancesResult},
        service::{
            BalanceItemErrorCode, BalanceItemOutcome, BalanceQuoteOutcome, BalanceTokenSelector,
            ResolvedBalanceTarget,
        },
    },
    domain::onchain_time::as_of::AsOf,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BalancesResponsePresenterError {
    ExpectedSingleAccount,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BalancesResponsePresenter;

impl BalancesResponsePresenter {
    pub(crate) fn single(
        &self,
        snapshot: GetBalancesResult,
    ) -> Result<SingleBalanceResponse, BalancesResponsePresenterError> {
        let mut accounts = snapshot.accounts;
        if accounts.len() != 1 {
            return Err(BalancesResponsePresenterError::ExpectedSingleAccount);
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

    pub(crate) fn bulk(&self, snapshot: GetBalancesResult) -> BulkBalanceResponse {
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

fn shape_account(account: BalancesAccountResult) -> ShapedAccount {
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

fn shape_as_of(as_of: &AsOf, evidence: Option<&BalanceEvidencePayload>) -> BalanceAsOfPayload {
    match as_of {
        AsOf::Latest => BalanceAsOfPayload {
            kind: "latest".to_string(),
            timestamp: None,
            block_number: None,
            observed_at: evidence.map(|evidence| evidence.observed_at.clone()),
        },
        AsOf::Timestamp { timestamp } => BalanceAsOfPayload {
            kind: "timestamp".to_string(),
            timestamp: Some(timestamp.clone()),
            block_number: None,
            observed_at: None,
        },
        AsOf::BlockNumber { block_number } => BalanceAsOfPayload {
            kind: "block_number".to_string(),
            timestamp: None,
            block_number: Some(block_number.clone()),
            observed_at: None,
        },
    }
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
