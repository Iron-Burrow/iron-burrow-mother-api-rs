use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::adapters::http::{
    error::ApiError,
    validation::{reject_unknown_fields, validate_asset_slugs, validate_contract_addresses},
};

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TokenSelectorRequest {
    #[serde(default)]
    pub(crate) asset_slugs: Vec<String>,
    #[serde(default)]
    pub(crate) contract_addresses: Vec<String>,
}

const TOKEN_FIELDS: [&str; 2] = ["asset_slugs", "contract_addresses"];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct TokenFilterResolutionDTO {
    pub requested: TokenSelectorRequest,
    pub resolved_contract_addresses: Vec<ResolvedTokenSelectorRequest>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct ResolvedTokenSelectorRequest {
    pub contract_address: String,
    pub asset_slug: Option<String>,
    pub symbol: Option<String>,
    pub decimals: Option<u8>,
    pub source: TokenFilterSourceDTO,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TokenFilterSourceDTO {
    AssetSlug,
    ContractAddress,
}

/// Validates the required `tokens` selector object for balance requests.
///
/// The balances endpoint requires a present JSON object containing at least one
/// token selector, either `asset_slugs` or `contract_addresses`. Missing,
/// non-object, unknown-field, or fully empty selectors are rejected.
pub(crate) fn validate_required_non_empty_tokens_object(
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

/// Validates the optional `tokens` filter for ERC-20 transfer searches.
///
/// Unlike the balances endpoint, ERC-20 transfer search does not require token
/// selectors. A missing, `null`, or empty filter means "search transfers without
/// token filtering." If a filter object is provided, unknown fields and malformed
/// selector values are still rejected.
pub(crate) fn validate_optional_token_filters(
    value: Option<&Value>,
) -> Result<Option<TokenSelectorRequest>, ApiError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(tokens)) => {
            reject_unknown_fields(tokens, &TOKEN_FIELDS)?;

            // For ERC-20 transfer search, empty token filters mean an
            // unfiltered transfer-log search, not an invalid request.
            Ok(Some(TokenSelectorRequest {
                asset_slugs: validate_asset_slugs(tokens.get("asset_slugs"))?,
                contract_addresses: validate_contract_addresses(tokens.get("contract_addresses"))?,
            }))
        }
        Some(_) => Err(ApiError::invalid_json()),
    }
}
