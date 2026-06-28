use axum::http::{header::CONTENT_TYPE, HeaderMap};

use super::types::JsonObject;
use crate::adapters::http::dto::erc20_transfers::{
    Erc20TransferDirection, Erc20TransferTokenFilters,
};
use crate::adapters::http::error::ApiError;
use crate::domain::validation::{is_asset_slug, is_evm_address};

use serde_json::Value;

const TOKEN_FIELDS: [&str; 2] = ["asset_slugs", "contract_addresses"];

pub(super) fn ensure_json_content_type(headers: &HeaderMap) -> Result<(), ApiError> {
    if headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(is_json_content_type)
    {
        Ok(())
    } else {
        Err(ApiError::invalid_json())
    }
}

fn is_json_content_type(value: &str) -> bool {
    let mime = value
        .split(';')
        .next()
        .map(str::trim)
        .unwrap_or_default()
        .to_ascii_lowercase();

    mime.strip_prefix("application/")
        .is_some_and(|subtype| subtype == "json" || subtype.ends_with("+json"))
}

pub(super) fn reject_unknown_fields(
    object: &JsonObject,
    allowed_fields: &[&str],
) -> Result<(), ApiError> {
    if object
        .keys()
        .any(|field| !allowed_fields.contains(&field.as_str()))
    {
        return Err(ApiError::unknown_field());
    }

    Ok(())
}

pub(super) fn validate_network_slug<S: AsRef<str>>(
    value: Option<&Value>,
    allowed_network_slugs: &[S],
) -> Result<String, ApiError> {
    let Some(Value::String(network_slug)) = value else {
        return Err(ApiError::missing_network_slug());
    };

    let network_slug = network_slug.trim();

    if network_slug.is_empty() {
        return Err(ApiError::missing_network_slug());
    }

    if allowed_network_slugs
        .iter()
        .any(|allowed| allowed.as_ref() == network_slug)
    {
        Ok(network_slug.to_owned())
    } else {
        Err(ApiError::transfer_unsupported_network())
    }
}

pub(super) fn validate_address(value: Option<&Value>) -> Result<String, ApiError> {
    let Some(Value::String(address)) = value else {
        return Err(ApiError::invalid_address());
    };

    if address.trim().is_empty() || !is_evm_address(address) {
        return Err(ApiError::invalid_address());
    }

    Ok(address.to_ascii_lowercase())
}

pub(super) fn validate_direction(
    value: Option<&Value>,
) -> Result<Erc20TransferDirection, ApiError> {
    match value {
        Some(Value::String(direction)) => match direction.as_str() {
            "any" => Ok(Erc20TransferDirection::Any),
            "from" => Ok(Erc20TransferDirection::From),
            "to" => Ok(Erc20TransferDirection::To),
            _ => Err(ApiError::invalid_direction()),
        },
        _ => Err(ApiError::invalid_direction()),
    }
}

pub(super) fn validate_tokens(
    value: Option<&Value>,
) -> Result<Option<Erc20TransferTokenFilters>, ApiError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(tokens)) => {
            reject_unknown_fields(tokens, &TOKEN_FIELDS)?;

            Ok(Some(Erc20TransferTokenFilters {
                asset_slugs: validate_asset_slugs(tokens.get("asset_slugs"))?,
                contract_addresses: validate_contract_addresses(tokens.get("contract_addresses"))?,
            }))
        }
        Some(_) => Err(ApiError::invalid_json()),
    }
}

pub(super) fn validate_asset_slugs(value: Option<&Value>) -> Result<Vec<String>, ApiError> {
    match value {
        None => Ok(Vec::new()),
        Some(Value::Array(asset_slugs)) => asset_slugs
            .iter()
            .map(|value| match value {
                Value::String(asset_slug) if is_asset_slug(asset_slug) => Ok(asset_slug.clone()),
                _ => Err(ApiError::invalid_asset_slug()),
            })
            .collect(),
        Some(_) => Err(ApiError::invalid_asset_slug()),
    }
}

pub(super) fn validate_contract_addresses(value: Option<&Value>) -> Result<Vec<String>, ApiError> {
    match value {
        None => Ok(Vec::new()),
        Some(Value::Array(contract_addresses)) => contract_addresses
            .iter()
            .map(|value| match value {
                Value::String(contract_address) if is_evm_address(contract_address) => {
                    Ok(contract_address.to_ascii_lowercase())
                }
                _ => Err(ApiError::invalid_contract_address()),
            })
            .collect(),
        Some(_) => Err(ApiError::invalid_contract_address()),
    }
}
