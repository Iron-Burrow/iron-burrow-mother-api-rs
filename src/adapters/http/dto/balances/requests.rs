use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::adapters::http::dto::accounts::{
    validate_account_array, validate_account_object, OnchainAccountRequest,
};
use crate::adapters::http::dto::assets::token_selector::{
    validate_tokens_object, TokenSelectorRequest,
};
use crate::adapters::http::dto::onchain_time::as_of::{validate_as_of_object, AsOfRequest};
use crate::adapters::http::validation::validate_required_string;
use crate::adapters::http::{
    error::ApiError, types::JsonObject, validation::reject_unknown_fields,
};

const RESERVED_NETWORK_ALIAS_FIELDS: [&str; 3] = ["chain", "chain_id", "chain_slug"];
const SUPPORTED_BALANCE_NETWORK_SLUGS: [&str; 2] = ["eth-mainnet", "base-mainnet"];
const SINGLE_BALANCE_FIELDS: [&str; 4] = ["as_of", "account", "quote_currency", "tokens"];
const BULK_BALANCE_FIELDS: [&str; 4] = ["as_of", "accounts", "quote_currency", "tokens"];

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SingleBalanceRequest {
    pub(crate) as_of: AsOfRequest,
    pub(crate) account: OnchainAccountRequest,
    pub(crate) quote_currency: String,
    pub(crate) tokens: TokenSelectorRequest,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BulkBalanceRequest {
    pub(crate) as_of: AsOfRequest,
    pub(crate) accounts: Vec<OnchainAccountRequest>,
    pub(crate) quote_currency: String,
    pub(crate) tokens: TokenSelectorRequest,
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
            tokens: validate_tokens_object(request.get("tokens"))?,
        })
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
            tokens: validate_tokens_object(request.get("tokens"))?,
        })
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
