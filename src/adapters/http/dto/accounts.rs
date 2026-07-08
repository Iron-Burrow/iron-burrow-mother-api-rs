use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::adapters::http::{
    error::ApiError,
    validation::{reject_unknown_fields, validate_address},
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
    supported_network_slugs: &[&str],
) -> Result<OnchainAccountRequest, ApiError> {
    let Some(Value::Object(account)) = value else {
        return Err(ApiError::invalid_request());
    };

    reject_unknown_fields(account, &ACCOUNT_FIELDS)?;

    Ok(OnchainAccountRequest {
        network_slug: validate_account_network_slug(
            account.get("network_slug"),
            supported_network_slugs,
        )?,
        address: validate_address(account.get("address"))?,
        client_ref: validate_client_ref(account.get("client_ref"))?,
    })
}

pub(crate) fn validate_account_array(
    value: Option<&Value>,
    supported_network_slugs: &[&str],
) -> Result<Vec<OnchainAccountRequest>, ApiError> {
    let Some(Value::Array(accounts)) = value else {
        return Err(ApiError::invalid_request());
    };

    accounts
        .iter()
        .map(|account| validate_account_object(Some(account), supported_network_slugs))
        .collect::<Result<Vec<OnchainAccountRequest>, ApiError>>()
}

fn validate_account_network_slug(
    value: Option<&Value>,
    supported_network_slugs: &[&str],
) -> Result<String, ApiError> {
    let Some(Value::String(network_slug)) = value else {
        return Err(ApiError::missing_network_slug());
    };

    let network_slug = network_slug.trim();
    if network_slug.is_empty() {
        return Err(ApiError::missing_network_slug());
    }

    if supported_network_slugs.contains(&network_slug) {
        Ok(network_slug.to_string())
    } else {
        Err(ApiError::unsupported_network())
    }
}

fn validate_client_ref(value: Option<&Value>) -> Result<Option<String>, ApiError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(client_ref)) => Ok(Some(client_ref.clone())),
        Some(_) => Err(ApiError::invalid_request()),
    }
}
