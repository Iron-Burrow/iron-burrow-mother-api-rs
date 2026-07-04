use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::adapters::http::dto::assets::token_selector::TokenSelectorRequest;
use crate::adapters::http::dto::onchain_time::as_of::AsOfRequest;
use crate::domain::accounts::OnchainAccount;
use crate::{
    adapters::http::{
        error::ApiError,
        types::JsonObject,
        validation::{reject_unknown_fields, validate_asset_slugs, validate_contract_addresses},
    },
    application::balances::service::{
        BalanceAccountResult, BalanceEvidence, BalanceItemErrorCode, BalanceItemOutcome,
        BalanceQuoteOutcome, BalanceSnapshotResult, BalanceTokenSelector, ResolvedBalanceTarget,
    },
};

#[allow(dead_code)]
pub(crate) mod examples;

const RESERVED_NETWORK_ALIAS_FIELDS: [&str; 3] = ["chain", "chain_id", "chain_slug"];
const SINGLE_BALANCE_FIELDS: [&str; 4] = ["as_of", "account", "quote_currency", "tokens"];
const BULK_BALANCE_FIELDS: [&str; 4] = ["as_of", "accounts", "quote_currency", "tokens"];
const AS_OF_FIELDS: [&str; 3] = ["kind", "timestamp", "block_number"];
const ACCOUNT_FIELDS: [&str; 3] = ["network_slug", "address", "client_ref"];
const TOKEN_FIELDS: [&str; 2] = ["asset_slugs", "contract_addresses"];

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SingleBalanceRequest {
    pub(crate) as_of: AsOfRequest,
    pub(crate) account: BalanceAccountRequest,
    pub(crate) quote_currency: String,
    pub(crate) tokens: TokenSelectorRequest,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BulkBalanceRequest {
    pub(crate) as_of: AsOfRequest,
    pub(crate) accounts: Vec<BalanceAccountRequest>,
    pub(crate) quote_currency: String,
    pub(crate) tokens: TokenSelectorRequest,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BalanceAccountRequest {
    pub(crate) network_slug: String,
    pub(crate) address: String,
    pub(crate) client_ref: Option<String>,
}

impl TryFrom<JsonObject> for SingleBalanceRequest {
    type Error = ApiError;

    fn try_from(request: JsonObject) -> Result<Self, Self::Error> {
        reject_reserved_alias_fields_in_object(&request)?;
        reject_unknown_fields(&request, &SINGLE_BALANCE_FIELDS)?;
        validate_as_of_object(request.get("as_of"))?;
        validate_account_object(request.get("account"))?;
        validate_tokens_object(request.get("tokens"))?;

        serde_json::from_value(Value::Object(request)).map_err(|_| ApiError::invalid_request())
    }
}

impl TryFrom<JsonObject> for BulkBalanceRequest {
    type Error = ApiError;

    fn try_from(request: JsonObject) -> Result<Self, Self::Error> {
        reject_reserved_alias_fields_in_object(&request)?;
        reject_unknown_fields(&request, &BULK_BALANCE_FIELDS)?;
        validate_as_of_object(request.get("as_of"))?;
        validate_account_array(request.get("accounts"))?;
        validate_tokens_object(request.get("tokens"))?;

        serde_json::from_value(Value::Object(request)).map_err(|_| ApiError::invalid_request())
    }
}

fn reject_reserved_alias_fields_in_object(object: &JsonObject) -> Result<(), ApiError> {
    if RESERVED_NETWORK_ALIAS_FIELDS
        .iter()
        .any(|field| object.contains_key(*field))
    {
        return Err(ApiError::invalid_request());
    }

    for value in object.values() {
        reject_reserved_alias_fields(value)?;
    }

    Ok(())
}

fn reject_reserved_alias_fields(value: &Value) -> Result<(), ApiError> {
    match value {
        Value::Object(object) => reject_reserved_alias_fields_in_object(object),
        Value::Array(values) => {
            for value in values {
                reject_reserved_alias_fields(value)?;
            }

            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_as_of_object(value: Option<&Value>) -> Result<(), ApiError> {
    let Some(Value::Object(as_of)) = value else {
        return Err(ApiError::invalid_request());
    };

    reject_unknown_fields(as_of, &AS_OF_FIELDS)?;
    validate_required_string(as_of.get("kind"))?;
    validate_optional_string(as_of.get("timestamp"))?;
    validate_optional_string(as_of.get("block_number"))
}

fn validate_required_string(value: Option<&Value>) -> Result<(), ApiError> {
    match value {
        Some(Value::String(_)) => Ok(()),
        _ => Err(ApiError::invalid_request()),
    }
}

fn validate_optional_string(value: Option<&Value>) -> Result<(), ApiError> {
    match value {
        None | Some(Value::String(_)) => Ok(()),
        Some(_) => Err(ApiError::invalid_request()),
    }
}

fn validate_account_object(value: Option<&Value>) -> Result<(), ApiError> {
    let Some(Value::Object(account)) = value else {
        return Err(ApiError::invalid_request());
    };

    reject_unknown_fields(account, &ACCOUNT_FIELDS)
}

fn validate_account_array(value: Option<&Value>) -> Result<(), ApiError> {
    let Some(Value::Array(accounts)) = value else {
        return Err(ApiError::invalid_request());
    };

    for account in accounts {
        validate_account_object(Some(account))?;
    }

    Ok(())
}

fn validate_tokens_object(value: Option<&Value>) -> Result<(), ApiError> {
    let Some(value) = value else {
        return Err(ApiError::empty_tokens());
    };
    let Value::Object(tokens) = value else {
        return Err(ApiError::invalid_request());
    };

    reject_unknown_fields(tokens, &TOKEN_FIELDS)?;

    let asset_slugs = validate_asset_slugs(tokens.get("asset_slugs"))?;
    let contract_addresses = validate_contract_addresses(tokens.get("contract_addresses"))?;

    if asset_slugs.is_empty() && contract_addresses.is_empty() {
        return Err(ApiError::empty_tokens());
    }

    Ok(())
}

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
        Ok(SingleBalanceResponse {
            ok: true,
            response_type: "balances".to_string(),
            status: account.status,
            as_of: SingleAsOfPayload {
                kind: "latest".to_string(),
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

        BulkBalanceResponse {
            ok: true,
            response_type: "balances_bulk".to_string(),
            status,
            as_of: BulkAsOfPayload {
                kind: "latest".to_string(),
            },
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
    as_of: SingleAsOfPayload,
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
    as_of: BulkAsOfPayload,
    quote_currency: String,
    summary: BalanceSummaryPayload,
    accounts: Vec<BalanceAccountPayload>,
    errors: Vec<BalanceErrorPayload>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct SingleAsOfPayload {
    kind: String,
    observed_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BulkAsOfPayload {
    kind: String,
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
            },
            observed_at: evidence.observed_at,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct BalanceBlockPayload {
    number: String,
    hash: String,
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
