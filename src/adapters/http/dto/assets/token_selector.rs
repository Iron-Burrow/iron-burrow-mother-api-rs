use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::adapters::http::{
    error::ApiError,
    validation::{reject_unknown_fields, validate_asset_slugs, validate_contract_addresses},
};

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TokenSelectorRequest {
    #[serde(default)]
    pub(crate) asset_slugs: Vec<String>,
    #[serde(default)]
    pub(crate) contract_addresses: Vec<String>,
}

const TOKEN_FIELDS: [&str; 2] = ["asset_slugs", "contract_addresses"];

pub(crate) fn validate_tokens_object(
    value: Option<&Value>,
) -> Result<TokenSelectorRequest, ApiError> {
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

    Ok(TokenSelectorRequest {
        asset_slugs,
        contract_addresses,
    })
}
