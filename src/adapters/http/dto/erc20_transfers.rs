use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::onchain_window::OnchainWindowDTO;
use crate::adapters::http::dto::onchain_window::validate_window;
use crate::adapters::http::error::ApiError;
use crate::adapters::http::types::JsonObject;
use crate::adapters::http::validation::{
    reject_unknown_fields, validate_address, validate_direction, validate_network_slug,
    validate_tokens,
};

const SUPPORTED_NETWORKS_SLUG: [&str; 1] = ["eth-mainnet"];
const TOP_LEVEL_FIELDS: [&str; 5] = ["network_slug", "address", "direction", "tokens", "window"];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Erc20TransferSearchRequest {
    pub network_slug: String,
    pub address: String,
    pub direction: Erc20TransferDirection,
    pub tokens: Option<Erc20TransferTokenFilters>,
    pub window: OnchainWindowDTO,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct Erc20TransferSearchResponse {
    pub ok: bool,
    #[serde(rename = "type")]
    pub response_type: String,
    pub network_slug: String,
    pub address: String,
    pub direction: Erc20TransferDirection,
    pub window: OnchainWindowDTO,
    pub token_filters: Erc20TransferTokenFilterResolution,
    pub transfers: Vec<Erc20TransferRow>,
    pub limits: Erc20TransferSearchLimits,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Erc20TransferDirection {
    Any,
    From,
    To,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Erc20TransferTokenFilters {
    #[serde(default)]
    pub asset_slugs: Vec<String>,
    #[serde(default)]
    pub contract_addresses: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct Erc20TransferTokenFilterResolution {
    pub requested: Erc20TransferTokenFilters,
    pub resolved_contract_addresses: Vec<ResolvedErc20TokenFilter>,
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
    pub direction: Erc20TransferDirection,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct Erc20TransferSearchLimits {
    pub max_rows: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct ResolvedErc20TokenFilter {
    pub contract_address: String,
    pub asset_slug: Option<String>,
    pub symbol: Option<String>,
    pub decimals: Option<u8>,
    pub source: Erc20TransferTokenFilterSource,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Erc20TransferTokenFilterSource {
    AssetSlug,
    ContractAddress,
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

pub(crate) fn validate_request(
    request: &JsonObject,
) -> Result<Erc20TransferSearchRequest, ApiError> {
    reject_unknown_fields(request, &TOP_LEVEL_FIELDS)?;

    Ok(Erc20TransferSearchRequest {
        network_slug: validate_network_slug(request.get("network_slug"), &SUPPORTED_NETWORKS_SLUG)?,
        address: validate_address(request.get("address"))?,
        direction: validate_direction(request.get("direction"))?,
        tokens: validate_tokens(request.get("tokens"))?,
        window: validate_window(request.get("window"))?,
    })
}
