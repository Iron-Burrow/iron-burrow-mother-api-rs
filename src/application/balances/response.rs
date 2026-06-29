use serde::Serialize;

use crate::domain::balance_catalog::BalanceTarget;

use super::service::{
    BalanceAccountResult, BalanceEvidence, BalanceItemErrorCode, BalanceItemOutcome,
    BalanceQuoteOutcome, BalanceSnapshotAccount, BalanceSnapshotResult,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct BalanceResponseAssembler;

impl BalanceResponseAssembler {
    pub fn single(
        &self,
        snapshot: BalanceSnapshotResult,
    ) -> Result<SingleBalanceResponse, BalanceResponseAssemblerError> {
        let mut accounts = snapshot.accounts;
        if accounts.len() != 1 {
            return Err(BalanceResponseAssemblerError::ExpectedSingleAccount);
        }

        let account = shape_account(accounts.pop().expect("single account length checked"));
        Ok(SingleBalanceResponse {
            ok: true,
            response_type: "balances",
            status: account.status,
            as_of: SingleAsOfPayload {
                kind: "latest",
                observed_at: account
                    .evidence
                    .as_ref()
                    .map(|evidence| evidence.observed_at.clone()),
            },
            quote_currency: snapshot.quote_currency,
            account: account.account,
            evidence: account.evidence,
            positions: account.positions,
            skipped: account.skipped,
            errors: account.errors,
        })
    }

    pub fn bulk(&self, snapshot: BalanceSnapshotResult) -> BulkBalanceResponse {
        let requested_accounts = snapshot.accounts.len();
        let requested_assets = snapshot.requested_asset_slugs.len();
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

        BulkBalanceResponse {
            ok: true,
            response_type: "balances_bulk",
            status,
            as_of: BulkAsOfPayload { kind: "latest" },
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
pub enum BalanceResponseAssemblerError {
    ExpectedSingleAccount,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BalanceResponseStatus {
    Complete,
    Partial,
    Failed,
}

#[derive(Debug, Serialize)]
pub struct SingleBalanceResponse {
    ok: bool,
    #[serde(rename = "type")]
    response_type: &'static str,
    status: BalanceResponseStatus,
    as_of: SingleAsOfPayload,
    quote_currency: String,
    account: BalanceAccountIdentityPayload,
    evidence: Option<BalanceEvidencePayload>,
    positions: Vec<BalancePositionPayload>,
    skipped: Vec<BalanceSkippedPayload>,
    errors: Vec<BalanceErrorPayload>,
}

#[derive(Debug, Serialize)]
pub struct BulkBalanceResponse {
    ok: bool,
    #[serde(rename = "type")]
    response_type: &'static str,
    status: BalanceResponseStatus,
    as_of: BulkAsOfPayload,
    quote_currency: String,
    summary: BalanceSummaryPayload,
    accounts: Vec<BalanceAccountPayload>,
    errors: Vec<BalanceErrorPayload>,
}

#[derive(Debug, Serialize)]
struct SingleAsOfPayload {
    kind: &'static str,
    observed_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct BulkAsOfPayload {
    kind: &'static str,
}

#[derive(Debug, Serialize)]
struct BalanceSummaryPayload {
    requested_accounts: usize,
    requested_assets: usize,
    requested_resolution_items: usize,
    positions_returned: usize,
    skipped_items: usize,
    failed_items: usize,
}

#[derive(Debug, Serialize)]
struct BalanceAccountPayload {
    status: BalanceResponseStatus,
    account: BalanceAccountIdentityPayload,
    evidence: Option<BalanceEvidencePayload>,
    positions: Vec<BalancePositionPayload>,
    skipped: Vec<BalanceSkippedPayload>,
    errors: Vec<BalanceErrorPayload>,
}

#[derive(Debug, Serialize)]
struct BalanceAccountIdentityPayload {
    network_slug: String,
    address: String,
    client_ref: Option<String>,
}

impl From<BalanceSnapshotAccount> for BalanceAccountIdentityPayload {
    fn from(account: BalanceSnapshotAccount) -> Self {
        Self {
            network_slug: account.network_slug,
            address: account.address,
            client_ref: account.client_ref,
        }
    }
}

#[derive(Debug, Serialize)]
struct BalanceEvidencePayload {
    source: &'static str,
    network_slug: String,
    block: BalanceBlockPayload,
    observed_at: String,
}

impl From<BalanceEvidence> for BalanceEvidencePayload {
    fn from(evidence: BalanceEvidence) -> Self {
        Self {
            source: "bigwig",
            network_slug: evidence.network_slug,
            block: BalanceBlockPayload {
                number: evidence.block_number,
                hash: evidence.block_hash,
            },
            observed_at: evidence.observed_at,
        }
    }
}

#[derive(Debug, Serialize)]
struct BalanceBlockPayload {
    number: String,
    hash: String,
}

#[derive(Debug, Serialize)]
struct BalancePositionPayload {
    network_slug: String,
    asset_slug: String,
    symbol: String,
    balance: BalanceAmountPayload,
    quote: BalanceQuotePayload,
}

#[derive(Debug, Serialize)]
struct BalanceAmountPayload {
    raw_amount: String,
    amount: String,
    decimals: u8,
}

#[derive(Debug, Serialize)]
struct BalanceQuotePayload {
    status: BalanceQuoteStatus,
    currency: Option<String>,
    unit_price: Option<String>,
    value: Option<String>,
    price_as_of: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum BalanceQuoteStatus {
    Available,
    Unavailable,
    Unsupported,
}

#[derive(Debug, Serialize)]
struct BalanceSkippedPayload {
    network_slug: String,
    asset_slug: String,
    reason: &'static str,
}

#[derive(Debug, Serialize)]
struct BalanceErrorPayload {
    network_slug: String,
    asset_slug: String,
    code: &'static str,
    message: &'static str,
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
                positions.push(BalancePositionPayload {
                    network_slug: target.network_slug,
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
                reason: "asset_not_supported_on_network",
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

fn shape_quote(
    target: &BalanceTarget,
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

fn error_payload(target: &BalanceTarget, code: BalanceItemErrorCode) -> BalanceErrorPayload {
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
        asset_slug: target.asset_slug.clone(),
        code,
        message,
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
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{
        application::balances::service::{BalanceQuoteOutcome, BalanceSnapshotAccount},
        domain::balance_catalog::BalanceTargetKind,
    };

    #[test]
    fn single_response_serializes_complete_shape_and_observation_time() {
        let response = BalanceResponseAssembler
            .single(snapshot(vec![resolved(
                "usdc",
                "450000000",
                "450.000000",
                BalanceQuoteOutcome::Available {
                    currency: "MXN".to_string(),
                    unit_price: "18.45".to_string(),
                    value: "8302.500000".to_string(),
                    price_as_of: "2026-06-17T11:59:59Z".to_string(),
                },
            )]))
            .unwrap();
        let value = serde_json::to_value(response).unwrap();

        assert_eq!(
            value,
            json!({
                "ok": true,
                "type": "balances",
                "status": "complete",
                "as_of": {
                    "kind": "latest",
                    "observed_at": "2026-06-17T12:00:00Z"
                },
                "quote_currency": "MXN",
                "account": {
                    "network_slug": "base-mainnet",
                    "address": "0x1111111111111111111111111111111111111111",
                    "client_ref": "treasury"
                },
                "evidence": {
                    "source": "bigwig",
                    "network_slug": "base-mainnet",
                    "block": {
                        "number": "123",
                        "hash": format!("0x{}", "a".repeat(64))
                    },
                    "observed_at": "2026-06-17T12:00:00Z"
                },
                "positions": [{
                    "network_slug": "base-mainnet",
                    "asset_slug": "usdc",
                    "symbol": "USDC",
                    "balance": {
                        "raw_amount": "450000000",
                        "amount": "450.000000",
                        "decimals": 6
                    },
                    "quote": {
                        "status": "available",
                        "currency": "MXN",
                        "unit_price": "18.45",
                        "value": "8302.500000",
                        "price_as_of": "2026-06-17T11:59:59Z"
                    }
                }],
                "skipped": [],
                "errors": []
            })
        );
    }

    #[test]
    fn unavailable_and_unsupported_quotes_preserve_positions_and_make_partial() {
        let response = BalanceResponseAssembler.bulk(snapshot(vec![
            resolved(
                "usdc",
                "1",
                "0.000001",
                BalanceQuoteOutcome::Unavailable {
                    code: BalanceItemErrorCode::PriceResolutionFailed,
                },
            ),
            resolved(
                "ethereum",
                "1",
                "0.000000000000000001",
                BalanceQuoteOutcome::Unsupported,
            ),
        ]));
        let value = serde_json::to_value(response).unwrap();

        assert_eq!(value["status"], "partial");
        assert_eq!(value["summary"]["positions_returned"], 2);
        assert_eq!(value["summary"]["failed_items"], 0);
        assert_eq!(
            value["accounts"][0]["positions"][0]["quote"]["status"],
            "unavailable"
        );
        assert_eq!(
            value["accounts"][0]["positions"][0]["quote"]["currency"],
            json!(null)
        );
        assert_eq!(
            value["accounts"][0]["positions"][1]["quote"]["status"],
            "unsupported"
        );
        assert_eq!(value["accounts"][0]["errors"].as_array().unwrap().len(), 1);
        assert_eq!(
            value["accounts"][0]["errors"][0]["code"],
            "price_resolution_failed"
        );
        assert_eq!(
            value["accounts"][0]["errors"][0]["message"],
            "Quote could not be resolved for this asset."
        );
    }

    #[test]
    fn skipped_only_is_complete_and_all_supported_failures_are_failed() {
        let skipped = BalanceResponseAssembler.bulk(snapshot(vec![BalanceItemOutcome::Skipped {
            network_slug: "base-mainnet".to_string(),
            asset_slug: "bitso-mxn".to_string(),
        }]));
        let skipped = serde_json::to_value(skipped).unwrap();
        assert_eq!(skipped["status"], "complete");
        assert_eq!(skipped["summary"]["skipped_items"], 1);
        assert_eq!(skipped["summary"]["failed_items"], 0);

        let failed = BalanceResponseAssembler.bulk(snapshot(vec![BalanceItemOutcome::Failed {
            target: target("usdc"),
            code: BalanceItemErrorCode::BalanceProviderUnavailable,
        }]));
        let failed = serde_json::to_value(failed).unwrap();
        assert_eq!(failed["status"], "failed");
        assert_eq!(failed["summary"]["failed_items"], 1);
        assert_eq!(
            failed["accounts"][0]["errors"][0]["message"],
            "Balance is temporarily unavailable for this asset on this network."
        );
    }

    #[test]
    fn bulk_status_is_partial_when_any_account_degrades_but_another_resolves() {
        let mut first = account_result(vec![resolved(
            "usdc",
            "1000000",
            "1.000000",
            available_quote(),
        )]);
        first.account.address = "0x1111111111111111111111111111111111111111".to_string();
        let mut second = account_result(vec![BalanceItemOutcome::Failed {
            target: target("usdc"),
            code: BalanceItemErrorCode::BalanceResolutionFailed,
        }]);
        second.account.address = "0x2222222222222222222222222222222222222222".to_string();
        let response = BalanceResponseAssembler.bulk(BalanceSnapshotResult {
            quote_currency: "MXN".to_string(),
            requested_asset_slugs: vec!["usdc".to_string()],
            accounts: vec![first, second],
        });
        let value = serde_json::to_value(response).unwrap();

        assert_eq!(value["status"], "partial");
        assert_eq!(value["accounts"][0]["status"], "complete");
        assert_eq!(value["accounts"][1]["status"], "failed");
        assert_eq!(value["as_of"], json!({"kind": "latest"}));
        assert!(value["as_of"].get("observed_at").is_none());
    }

    #[test]
    fn single_without_evidence_serializes_null_observed_at_and_evidence() {
        let mut snapshot = snapshot(vec![BalanceItemOutcome::Skipped {
            network_slug: "base-mainnet".to_string(),
            asset_slug: "bitso-mxn".to_string(),
        }]);
        snapshot.accounts[0].evidence = None;
        let response = BalanceResponseAssembler.single(snapshot).unwrap();
        let value = serde_json::to_value(response).unwrap();

        assert_eq!(value["as_of"]["observed_at"], json!(null));
        assert_eq!(value["evidence"], json!(null));
    }

    fn snapshot(items: Vec<BalanceItemOutcome>) -> BalanceSnapshotResult {
        BalanceSnapshotResult {
            quote_currency: "MXN".to_string(),
            requested_asset_slugs: vec!["usdc".to_string(), "ethereum".to_string()],
            accounts: vec![account_result(items)],
        }
    }

    fn account_result(items: Vec<BalanceItemOutcome>) -> BalanceAccountResult {
        BalanceAccountResult {
            account: BalanceSnapshotAccount {
                network_slug: "base-mainnet".to_string(),
                address: "0x1111111111111111111111111111111111111111".to_string(),
                client_ref: Some("treasury".to_string()),
            },
            evidence: Some(BalanceEvidence {
                network_slug: "base-mainnet".to_string(),
                observed_at: "2026-06-17T12:00:00Z".to_string(),
                block_number: "123".to_string(),
                block_hash: format!("0x{}", "a".repeat(64)),
            }),
            items,
        }
    }

    fn resolved(
        asset_slug: &str,
        raw_amount: &str,
        amount: &str,
        quote: BalanceQuoteOutcome,
    ) -> BalanceItemOutcome {
        BalanceItemOutcome::Resolved {
            target: target(asset_slug),
            raw_amount: raw_amount.to_string(),
            amount: amount.to_string(),
            quote,
        }
    }

    fn available_quote() -> BalanceQuoteOutcome {
        BalanceQuoteOutcome::Available {
            currency: "MXN".to_string(),
            unit_price: "18.45".to_string(),
            value: "18.450000".to_string(),
            price_as_of: "2026-06-17T11:59:59Z".to_string(),
        }
    }

    fn target(asset_slug: &str) -> BalanceTarget {
        BalanceTarget {
            network_slug: "base-mainnet".to_string(),
            chain_id: 8453,
            asset_slug: asset_slug.to_string(),
            symbol: asset_slug.to_ascii_uppercase(),
            name: format!("{asset_slug} display name"),
            decimals: if asset_slug == "ethereum" { 18 } else { 6 },
            pricing_asset_slug: asset_slug.to_string(),
            kind: BalanceTargetKind::Native,
        }
    }
}
