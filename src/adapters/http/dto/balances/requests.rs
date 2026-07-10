use std::collections::HashSet;

use axum::body::Bytes;
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::adapters::http::dto::accounts::{
    validate_account_array, validate_account_object, OnchainAccountRequest,
};
use crate::adapters::http::dto::assets::token_selector::{
    validate_required_non_empty_tokens_object, TokenSelectorRequest,
};
use crate::adapters::http::dto::onchain_time::as_of::{validate_as_of_object, AsOfRequest};
use crate::adapters::http::json_body::parse_json_object_body;
use crate::adapters::http::validation::{ensure_json_content_type, validate_required_string};
use crate::adapters::http::{
    error::ApiError, types::JsonObject, validation::reject_unknown_fields,
};
use crate::application::balances::command::{
    GetBalancesCommand, MAX_ACCOUNTS, MAX_RESOLUTION_ITEMS, MAX_TOKENS,
};
use crate::domain::accounts::OnchainAccount;
use crate::domain::assets::token_selector::TokenSelector;
use crate::domain::onchain_time::as_of::AsOf;
use crate::domain::validation::is_evm_address;

use super::error::command_error_to_api_error;

const RESERVED_NETWORK_ALIAS_FIELDS: [&str; 3] = ["chain", "chain_id", "chain_slug"];
const SUPPORTED_BALANCE_NETWORK_SLUGS: [&str; 2] = ["eth-mainnet", "base-mainnet"];
const SINGLE_BALANCE_FIELDS: [&str; 4] = ["as_of", "account", "quote_currency", "tokens"];
const BULK_BALANCE_FIELDS: [&str; 4] = ["as_of", "accounts", "quote_currency", "tokens"];

/// -----------------------
/// Single Balance Request
/// -----------------------

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SingleBalanceRequest {
    pub(crate) as_of: AsOfRequest,
    pub(crate) account: OnchainAccountRequest,
    pub(crate) quote_currency: String,
    pub(crate) tokens: TokenSelectorRequest,
}

impl TryFrom<(&HeaderMap, &Bytes)> for SingleBalanceRequest {
    type Error = ApiError;

    fn try_from((headers, body): (&HeaderMap, &Bytes)) -> Result<Self, Self::Error> {
        ensure_json_content_type(headers).map_err(|_| ApiError::invalid_request())?;
        let request = parse_json_object_body(body).map_err(|_| ApiError::invalid_request())?;

        SingleBalanceRequest::try_from(request)
    }
}

impl TryFrom<JsonObject> for SingleBalanceRequest {
    type Error = ApiError;

    fn try_from(request: JsonObject) -> Result<Self, Self::Error> {
        reject_reserved_alias_fields_in_object(&request)?;
        reject_unknown_fields(&request, &SINGLE_BALANCE_FIELDS)?;

        Ok(Self {
            as_of: validate_as_of_object(request.get("as_of"))?,
            account: validate_account_object(
                request.get("account"),
                &SUPPORTED_BALANCE_NETWORK_SLUGS,
            )?,
            quote_currency: validate_required_string(request.get("quote_currency"))?,
            tokens: validate_required_non_empty_tokens_object(request.get("tokens"))?,
        })
    }
}

impl TryFrom<SingleBalanceRequest> for GetBalancesCommand {
    type Error = ApiError;

    fn try_from(request: SingleBalanceRequest) -> Result<Self, Self::Error> {
        command_from_request_parts(
            request.as_of,
            vec![request.account],
            request.quote_currency,
            request.tokens,
        )
    }
}

/// ---------------------
/// Bulk Balance Request
/// ---------------------

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BulkBalanceRequest {
    pub(crate) as_of: AsOfRequest,
    pub(crate) accounts: Vec<OnchainAccountRequest>,
    pub(crate) quote_currency: String,
    pub(crate) tokens: TokenSelectorRequest,
}

impl TryFrom<(&HeaderMap, &Bytes)> for BulkBalanceRequest {
    type Error = ApiError;

    fn try_from((headers, body): (&HeaderMap, &Bytes)) -> Result<Self, Self::Error> {
        ensure_json_content_type(headers).map_err(|_| ApiError::invalid_request())?;
        let request = parse_json_object_body(body).map_err(|_| ApiError::invalid_request())?;

        BulkBalanceRequest::try_from(request)
    }
}

impl TryFrom<JsonObject> for BulkBalanceRequest {
    type Error = ApiError;

    fn try_from(request: JsonObject) -> Result<Self, Self::Error> {
        reject_reserved_alias_fields_in_object(&request)?;
        reject_unknown_fields(&request, &BULK_BALANCE_FIELDS)?;

        Ok(Self {
            as_of: validate_as_of_object(request.get("as_of"))?,
            accounts: validate_account_array(
                request.get("accounts"),
                &SUPPORTED_BALANCE_NETWORK_SLUGS,
            )?,
            quote_currency: validate_required_string(request.get("quote_currency"))?,
            tokens: validate_required_non_empty_tokens_object(request.get("tokens"))?,
        })
    }
}

impl TryFrom<BulkBalanceRequest> for GetBalancesCommand {
    type Error = ApiError;

    fn try_from(request: BulkBalanceRequest) -> Result<Self, Self::Error> {
        command_from_request_parts(
            request.as_of,
            request.accounts,
            request.quote_currency,
            request.tokens,
        )
    }
}

fn command_from_request_parts(
    as_of: AsOfRequest,
    accounts: Vec<OnchainAccountRequest>,
    quote_currency: String,
    tokens: TokenSelectorRequest,
) -> Result<GetBalancesCommand, ApiError> {
    let as_of = AsOf::try_from(as_of)?;

    let accounts = accounts
        .into_iter()
        .map(|account| OnchainAccount {
            network_slug: account.network_slug,
            address: account.address,
            client_ref: account.client_ref,
        })
        .collect();

    let tokens = TokenSelector {
        asset_slugs: tokens.asset_slugs,
        contract_addresses: tokens.contract_addresses,
    };

    GetBalancesCommand::try_new(as_of, accounts, quote_currency, tokens)
        .map_err(command_error_to_api_error)
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

#[cfg(test)]
mod tests {
    use axum::response::IntoResponse;
    use reqwest::StatusCode;

    use super::*;

    const ACCOUNT_A: &str = "0x1111111111111111111111111111111111111111";

    fn latest_as_of() -> AsOfRequest {
        AsOfRequest {
            kind: "latest".to_string(),
            timestamp: None,
            block_number: None,
        }
    }

    fn account(network_slug: &str, address: &str) -> OnchainAccountRequest {
        OnchainAccountRequest {
            network_slug: network_slug.to_string(),
            address: address.to_string(),
            client_ref: None,
        }
    }

    fn tokens<const N: usize>(asset_slugs: [&str; N]) -> TokenSelectorRequest {
        token_vec(asset_slugs.into_iter().map(str::to_string).collect())
    }

    fn token_vec(asset_slugs: Vec<String>) -> TokenSelectorRequest {
        TokenSelectorRequest {
            asset_slugs,
            contract_addresses: Vec::new(),
        }
    }

    #[test]
    fn validation_normalizes_quote_and_preserves_public_identifiers() {
        let request = command_from_request_parts(
            latest_as_of(),
            vec![account("eth-mainnet", ACCOUNT_A)],
            " mxn ".to_string(),
            tokens(["ethereum"]),
        )
        .unwrap();

        assert_eq!(request.view().quote_currency, "MXN");
        assert_eq!(request.view().accounts[0].network_slug, "eth-mainnet");
        assert_eq!(request.view().accounts[0].address, ACCOUNT_A);
        assert_eq!(request.view().tokens.asset_slugs, ["ethereum"]);
        assert!(request.view().tokens.contract_addresses.is_empty());
    }

    #[test]
    fn validation_allows_same_address_on_different_networks() {
        let request = command_from_request_parts(
            latest_as_of(),
            vec![
                account("eth-mainnet", ACCOUNT_A),
                account("base-mainnet", ACCOUNT_A),
            ],
            "USD".to_string(),
            tokens(["usdc"]),
        )
        .unwrap();

        assert_eq!(request.view().accounts.len(), 2);
    }

    #[test]
    fn validation_rejects_duplicate_accounts_case_insensitively() {
        let error = command_from_request_parts(
            latest_as_of(),
            vec![
                account("eth-mainnet", "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd"),
                account("eth-mainnet", "0xABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCD"),
            ],
            "USD".to_string(),
            tokens(["usdc"]),
        )
        .unwrap_err();

        assert_eq!(
            error.into_response().status(),
            axum::http::StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn request_parts_are_mapped_into_command() {
        let accounts = vec![
            account("eth-mainnet", "0x1111111111111111111111111111111111111111"),
            account("base-mainnet", "0x2222222222222222222222222222222222222222"),
        ];

        let command = command_from_request_parts(
            latest_as_of(),
            accounts,
            "USD".to_string(),
            tokens(["ethereum", "usdc"]),
        )
        .unwrap();

        assert_eq!(command.view().accounts.len(), 2);
        assert_eq!(command.view().tokens.asset_slugs, ["ethereum", "usdc"]);
    }

    #[test]
    fn validation_deduplicates_explicit_contracts_case_insensitively_before_limits() {
        let request = command_from_request_parts(
            latest_as_of(),
            vec![account("eth-mainnet", ACCOUNT_A)],
            "USD".to_string(),
            TokenSelectorRequest {
                asset_slugs: Vec::new(),
                contract_addresses: vec![
                    "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
                    "0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48".to_string(),
                ],
            },
        )
        .unwrap();

        assert_eq!(
            request.view().tokens.contract_addresses,
            ["0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"]
        );
        assert!(request.view().tokens.asset_slugs.is_empty());
    }

    #[test]
    fn maps_account_limit_error_to_request_too_large() {
        let accounts = (0..=MAX_ACCOUNTS)
            .map(|index| account("eth-mainnet", &format!("0x{index:040x}")))
            .collect();

        let error = command_from_request_parts(
            latest_as_of(),
            accounts,
            "USD".to_string(),
            tokens(["ethereum"]),
        )
        .unwrap_err();

        assert_eq!(
            error.into_response().status(),
            StatusCode::PAYLOAD_TOO_LARGE,
        );
    }
}
