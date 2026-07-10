use crate::{
    adapters::http::dto::balances::{
        BalanceAccountPayload, BalanceAmountPayload, BalanceAsOfPayload, BalanceErrorPayload,
        BalanceEvidencePayload, BalancePositionPayload, BalanceQuotePayload, BalanceQuoteStatus,
        BalanceResponseStatus, BalanceSelectorPayload, BalanceSkippedPayload,
        BalanceSummaryPayload, BulkBalanceResponse, SingleBalanceResponse,
    },
    application::balances::{
        error::BalanceItemErrorCode,
        result::{
            BalanceItemOutcome, BalanceQuoteOutcome, BalanceTokenSelector, BalancesAccountResult,
            GetBalancesResult, ResolvedBalanceTarget,
        },
    },
    domain::{assets::balance_catalog::BalanceTargetKind, onchain_time::as_of::AsOf},
};

struct PresentedAccount {
    payload: BalanceAccountPayload,
    stats: AccountPresentationStats,
}

#[derive(Clone, Copy, Debug, Default)]
struct AccountPresentationStats {
    supported_balance_items: usize,
    resolved_balance_items: usize,
    failed_balance_items: usize,
    degraded_quote: bool,
}

impl AccountPresentationStats {
    fn record_resolved(&mut self, degraded_quote: bool) {
        self.supported_balance_items += 1;
        self.resolved_balance_items += 1;
        self.degraded_quote |= degraded_quote;
    }

    fn record_failed(&mut self) {
        self.supported_balance_items += 1;
        self.failed_balance_items += 1;
    }

    fn status(&self) -> BalanceResponseStatus {
        if self.supported_balance_items == 0 {
            BalanceResponseStatus::Complete
        } else if self.resolved_balance_items == 0 {
            BalanceResponseStatus::Failed
        } else if self.failed_balance_items > 0 || self.degraded_quote {
            BalanceResponseStatus::Partial
        } else {
            BalanceResponseStatus::Complete
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BalancesResponsePresenterError {
    ExpectedSingleAccount,
}

struct PresentedQuote {
    payload: BalanceQuotePayload,
    error: Option<BalanceErrorPayload>,
}

#[derive(Clone, Copy, Debug, Default)]
struct BulkPresentationStats {
    supported_balance_items: usize,
    resolved_balance_items: usize,
    failed_balance_items: usize,
    positions_returned: usize,
    skipped_items: usize,
    has_degraded_account: bool,
}

impl BulkPresentationStats {
    fn from_accounts(accounts: &[PresentedAccount]) -> Self {
        accounts.iter().fold(Self::default(), |mut stats, account| {
            stats.supported_balance_items += account.stats.supported_balance_items;

            stats.resolved_balance_items += account.stats.resolved_balance_items;

            stats.failed_balance_items += account.stats.failed_balance_items;

            stats.positions_returned += account.payload.positions.len();

            stats.skipped_items += account.payload.skipped.len();

            stats.has_degraded_account |= account.payload.status != BalanceResponseStatus::Complete;

            stats
        })
    }

    fn status(&self) -> BalanceResponseStatus {
        if self.supported_balance_items == 0 {
            BalanceResponseStatus::Complete
        } else if self.resolved_balance_items == 0 {
            BalanceResponseStatus::Failed
        } else if self.has_degraded_account {
            BalanceResponseStatus::Partial
        } else {
            BalanceResponseStatus::Complete
        }
    }

    fn summary(&self, requested_accounts: usize, requested_assets: usize) -> BalanceSummaryPayload {
        BalanceSummaryPayload {
            requested_accounts,
            requested_assets,
            requested_resolution_items: requested_accounts.saturating_mul(requested_assets),
            positions_returned: self.positions_returned,
            skipped_items: self.skipped_items,
            failed_items: self.failed_balance_items,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BalancesResponsePresenter;

impl BalancesResponsePresenter {
    pub(crate) fn single(
        &self,
        result: GetBalancesResult,
    ) -> Result<SingleBalanceResponse, BalancesResponsePresenterError> {
        let mut accounts = result.accounts;
        if accounts.len() != 1 {
            return Err(BalancesResponsePresenterError::ExpectedSingleAccount);
        }

        let account = present_account(accounts.pop().expect("single account length checked"));
        let as_of = shape_as_of(&result.as_of, account.payload.evidence.as_ref());
        Ok(SingleBalanceResponse {
            ok: true,
            response_type: "balances".to_string(),
            status: account.payload.status,
            as_of,
            quote_currency: result.quote_currency,
            account: account.payload.account,
            evidence: account.payload.evidence,
            positions: account.payload.positions,
            skipped: account.payload.skipped,
            errors: account.payload.errors,
        })
    }

    pub(crate) fn bulk(&self, result: GetBalancesResult) -> BulkBalanceResponse {
        let requested_accounts = result.accounts.len();
        let requested_assets = result.requested_token_count;

        let accounts = result
            .accounts
            .into_iter()
            .map(present_account)
            .collect::<Vec<_>>();

        let stats = BulkPresentationStats::from_accounts(&accounts);

        BulkBalanceResponse {
            ok: true,
            response_type: "balances_bulk".to_string(),
            status: stats.status(),
            as_of: shape_as_of(&result.as_of, None),
            quote_currency: result.quote_currency,
            summary: stats.summary(requested_accounts, requested_assets),
            accounts: accounts
                .into_iter()
                .map(|account| account.payload)
                .collect(),
            errors: Vec::new(),
        }
    }
}

fn present_account(account: BalancesAccountResult) -> PresentedAccount {
    let mut positions = Vec::new();
    let mut skipped = Vec::new();
    let mut errors = Vec::new();
    let mut stats = AccountPresentationStats::default();

    for item in account.items {
        match item {
            BalanceItemOutcome::Resolved {
                target,
                raw_amount,
                amount,
                quote,
            } => {
                let PresentedQuote {
                    payload: quote,
                    error,
                } = present_quote(&target, quote);

                stats.record_resolved(quote.status != BalanceQuoteStatus::Available);

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
                stats.record_failed();
                errors.push(error_payload(&target, code));
            }
        }
    }

    PresentedAccount {
        payload: BalanceAccountPayload {
            status: stats.status(),
            account: account.account.into(),
            evidence: account.evidence.map(BalanceEvidencePayload::from),
            positions,
            skipped,
            errors,
        },
        stats,
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

fn present_quote(target: &ResolvedBalanceTarget, quote: BalanceQuoteOutcome) -> PresentedQuote {
    match quote {
        BalanceQuoteOutcome::Available {
            currency,
            unit_price,
            value,
            price_as_of,
        } => PresentedQuote {
            payload: BalanceQuotePayload {
                status: BalanceQuoteStatus::Available,
                currency: Some(currency),
                unit_price: Some(unit_price),
                value: Some(value),
                price_as_of: Some(price_as_of),
            },
            error: None,
        },
        BalanceQuoteOutcome::Unavailable { code } => PresentedQuote {
            payload: BalanceQuotePayload {
                status: BalanceQuoteStatus::Unavailable,
                currency: None,
                unit_price: None,
                value: None,
                price_as_of: None,
            },
            error: Some(error_payload(target, code)),
        },
        BalanceQuoteOutcome::Unsupported => PresentedQuote {
            payload: BalanceQuotePayload {
                status: BalanceQuoteStatus::Unsupported,
                currency: None,
                unit_price: None,
                value: None,
                price_as_of: None,
            },
            error: None,
        },
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
        BalanceTargetKind::Native => None,
        BalanceTargetKind::Erc20 { contract_address } => Some(contract_address.clone()),
    }
}
