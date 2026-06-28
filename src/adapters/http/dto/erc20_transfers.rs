use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
const TOP_LEVEL_FIELDS: [&str; 5] = ["network_slug", "address", "direction", "tokens", "window"];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Erc20TransferSearchRequest {
    pub network_slug: String,
    pub address: String,
    pub direction: TransferDirectionDTO,
    pub tokens: Option<TokenFilterDTO>,
    pub window: OnchainWindowDTO,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct Erc20TransferSearchResponse {
    pub ok: bool,
    #[serde(rename = "type")]
    pub response_type: String,
    pub network_slug: String,
    pub address: String,
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

        Ok(Self {
            network_slug: validate_network_slug(
                request.get("network_slug"),
                &SUPPORTED_NETWORKS_SLUG,
            )?,
            address: validate_address(request.get("address"))?,
            direction: validate_direction(request.get("direction"))?,
            tokens: validate_tokens(request.get("tokens"))?,
            window: validate_window(request.get("window"))?,
        })
    }
}

#[cfg(test)]
mod tests;
