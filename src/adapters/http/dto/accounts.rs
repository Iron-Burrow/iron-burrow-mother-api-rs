use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::adapters::http::{
    error::ApiError,
    validation::{reject_unknown_fields, validate_optional_string, validate_required_string},
};

const ACCOUNT_FIELDS: [&str; 3] = ["network_slug", "address", "client_ref"];

#[derive(Clone, Debug, Eq, Deserialize, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OnchainAccountRequest {
    pub(crate) network_slug: String,
    pub(crate) address: String,
    pub(crate) client_ref: Option<String>,
}

#[derive(Clone, Debug, Eq, Deserialize, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OnchainAccountResponse {
    pub(crate) network_slug: String,
    pub(crate) address: String,
    pub(crate) client_ref: Option<String>,
}

pub(crate) fn validate_account_object(
    value: Option<&Value>,
) -> Result<OnchainAccountRequest, ApiError> {
    let Some(Value::Object(account)) = value else {
        return Err(ApiError::invalid_request());
    };

    reject_unknown_fields(account, &ACCOUNT_FIELDS)?;

    Ok(OnchainAccountRequest {
        network_slug: validate_required_string(account.get("network_slug"))?,
        address: validate_required_string(account.get("address"))?,
        client_ref: validate_optional_string(account.get("client_ref"))?,
    })
}

pub(crate) fn validate_account_array(
    value: Option<&Value>,
) -> Result<Vec<OnchainAccountRequest>, ApiError> {
    let Some(Value::Array(accounts)) = value else {
        return Err(ApiError::invalid_request());
    };

    Ok(accounts
        .iter()
        .map(|account| validate_account_object(Some(account)))
        .collect::<Result<Vec<OnchainAccountRequest>, ApiError>>()?)
}
