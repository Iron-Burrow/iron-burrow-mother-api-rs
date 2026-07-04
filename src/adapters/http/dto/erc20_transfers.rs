use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub(crate) mod examples;
pub(crate) mod requests;

use super::filters::onchain_window::OnchainWindowDTO;
use crate::adapters::http::dto::accounts::{OnchainAccountRequest, OnchainAccountResponse};
use crate::adapters::http::dto::filters::token_filters::TokenFilterResolutionDTO;
use crate::adapters::http::dto::filters::transfer_direction::TransferDirectionDTO;
use crate::adapters::http::error::ApiError;
use crate::adapters::http::validation::{
    reject_unknown_fields, validate_address, validate_network_slug,
};

const SUPPORTED_NETWORKS_SLUG: [&str; 1] = ["eth-mainnet"];
const ACCOUNT_FIELDS: [&str; 3] = ["network_slug", "address", "client_ref"];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct Erc20TransferSearchResponse {
    pub ok: bool,
    #[serde(rename = "type")]
    pub response_type: String,
    pub account: OnchainAccountResponse,
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

fn validate_account(value: Option<&serde_json::Value>) -> Result<OnchainAccountRequest, ApiError> {
    let Some(serde_json::Value::Object(account)) = value else {
        return Err(ApiError::missing_network_slug());
    };

    reject_unknown_fields(account, &ACCOUNT_FIELDS)?;

    Ok(OnchainAccountRequest {
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
