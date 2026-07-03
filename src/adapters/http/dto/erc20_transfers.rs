use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub(crate) mod examples;

use super::filters::onchain_window::{validate_window, OnchainWindowDTO};
use super::filters::transfer_direction::validate_direction;
use crate::adapters::http::dto::filters::token_filters::{
    validate_tokens, TokenFilterDTO, TokenFilterResolutionDTO,
};
use crate::adapters::http::dto::filters::transfer_direction::TransferDirectionDTO;
use crate::adapters::http::error::ApiError;
use crate::adapters::http::types::JsonObject;
use crate::adapters::http::validation::{
    reject_unknown_fields, validate_address, validate_network_slug,
};

const SUPPORTED_NETWORKS_SLUG: [&str; 1] = ["eth-mainnet"];
const TOP_LEVEL_FIELDS: [&str; 4] = ["account", "direction", "tokens", "window"];
const ACCOUNT_FIELDS: [&str; 3] = ["network_slug", "address", "client_ref"];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Erc20TransferSearchRequest {
    pub account: Erc20TransferAccount,
    pub direction: TransferDirectionDTO,
    pub tokens: Option<TokenFilterDTO>,
    pub window: OnchainWindowDTO,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Erc20TransferAccount {
    pub network_slug: String,
    pub address: String,
    pub client_ref: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct Erc20TransferSearchResponse {
    pub ok: bool,
    #[serde(rename = "type")]
    pub response_type: String,
    pub account: Erc20TransferAccount,
    pub direction: TransferDirectionDTO,
    pub window: OnchainWindowDTO,
    pub token_filters: TokenFilterResolutionDTO,
    pub transfers: Vec<Erc20TransferRow>,
    pub limits: Erc20TransferSearchLimits,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct Erc20TransferRow {
    pub block_number: u64,
    pub tx_hash: String,
    pub log_index: u64,
    pub token: Erc20TransferToken,
    pub from: String,
    pub to: String,
    pub amount: Erc20TransferAmount,
    pub direction: TransferDirectionDTO,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct Erc20TransferSearchLimits {
    pub max_rows: u64,
    pub truncated: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct Erc20TransferToken {
    pub contract_address: String,
    pub asset_slug: Option<String>,
    pub symbol: Option<String>,
    pub decimals: Option<u8>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct Erc20TransferAmount {
    pub raw: String,
    pub decimal: Option<String>,
}

impl TryFrom<&JsonObject> for Erc20TransferSearchRequest {
    type Error = ApiError;

    fn try_from(request: &JsonObject) -> Result<Self, Self::Error> {
        reject_unknown_fields(request, &TOP_LEVEL_FIELDS)?;
        let account = validate_account(request.get("account"))?;

        Ok(Self {
            account,
            direction: validate_direction(request.get("direction"))?,
            tokens: validate_tokens(request.get("tokens"))?,
            window: validate_window(request.get("window"))?,
        })
    }
}

fn validate_account(value: Option<&serde_json::Value>) -> Result<Erc20TransferAccount, ApiError> {
    let Some(serde_json::Value::Object(account)) = value else {
        return Err(ApiError::missing_network_slug());
    };

    reject_unknown_fields(account, &ACCOUNT_FIELDS)?;

    Ok(Erc20TransferAccount {
        network_slug: validate_network_slug(account.get("network_slug"), &SUPPORTED_NETWORKS_SLUG)?,
        address: validate_address(account.get("address"))?,
        client_ref: match account.get("client_ref") {
            None | Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(client_ref)) => Some(client_ref.clone()),
            Some(_) => return Err(ApiError::invalid_request()),
        },
    })
}

#[cfg(test)]
mod tests;
