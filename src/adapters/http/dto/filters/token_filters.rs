use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::adapters::http::error::ApiError;
use crate::adapters::http::validation::{
    reject_unknown_fields, validate_asset_slugs, validate_contract_addresses,
};

const TOKEN_FIELDS: [&str; 2] = ["asset_slugs", "contract_addresses"];

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TokenFilterDTO {
    #[serde(default)]
    pub asset_slugs: Vec<String>,
    #[serde(default)]
    pub contract_addresses: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct TokenFilterResolutionDTO {
    pub requested: TokenFilterDTO,
    pub resolved_contract_addresses: Vec<ResolvedTokenFilterDTO>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub(crate) struct ResolvedTokenFilterDTO {
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

pub(crate) fn validate_tokens(value: Option<&Value>) -> Result<Option<TokenFilterDTO>, ApiError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(tokens)) => {
            reject_unknown_fields(tokens, &TOKEN_FIELDS)?;

            Ok(Some(TokenFilterDTO {
                asset_slugs: validate_asset_slugs(tokens.get("asset_slugs"))?,
                contract_addresses: validate_contract_addresses(tokens.get("contract_addresses"))?,
            }))
        }
        Some(_) => Err(ApiError::invalid_json()),
    }
}
