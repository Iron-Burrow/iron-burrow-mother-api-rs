use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::adapters::http::dto::accounts::OnchainAccountResponse;
use crate::adapters::http::dto::filters::onchain_window::OnchainWindowDTO;
use crate::adapters::http::dto::filters::token_filters::TokenFilterResolutionDTO;
use crate::adapters::http::dto::filters::transfer_direction::TransferDirectionDTO;

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
