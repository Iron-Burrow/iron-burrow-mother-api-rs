use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::adapters::http::dto::assets::token_selector::{
    validate_optional_token_filters, TokenSelectorRequest,
};
use crate::adapters::http::dto::{
    accounts::{validate_account_object, OnchainAccountRequest},
    onchain_time::onchain_window::{validate_window, OnchainWindowDTO},
    transfers::transfer_direction::{validate_direction, TransferDirectionDTO},
};
use crate::adapters::http::error::ApiError;
use crate::adapters::http::types::JsonObject;
use crate::adapters::http::validation::reject_unknown_fields;

const SUPPORTED_ERC20_TRANSFER_NETWORK_SLUGS: [&str; 1] = ["eth-mainnet"];
const TOP_LEVEL_FIELDS: [&str; 4] = ["account", "direction", "tokens", "window"];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Erc20TransferSearchRequest {
    pub account: OnchainAccountRequest,
    pub direction: TransferDirectionDTO,
    pub tokens: Option<TokenSelectorRequest>,
    pub window: OnchainWindowDTO,
}

impl TryFrom<&JsonObject> for Erc20TransferSearchRequest {
    type Error = ApiError;

    fn try_from(request: &JsonObject) -> Result<Self, Self::Error> {
        reject_unknown_fields(request, &TOP_LEVEL_FIELDS)?;
        let account = validate_account_object(
            request.get("account"),
            &SUPPORTED_ERC20_TRANSFER_NETWORK_SLUGS,
        )?;

        Ok(Self {
            account,
            direction: validate_direction(request.get("direction"))?,
            tokens: validate_optional_token_filters(request.get("tokens"))?,
            window: validate_window(request.get("window"))?,
        })
    }
}
