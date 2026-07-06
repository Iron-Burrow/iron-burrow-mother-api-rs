use axum::http::{header::CONTENT_TYPE, HeaderMap};
use serde_json::Value;

use super::types::JsonObject;
use crate::adapters::http::error::ApiError;
use crate::domain::validation::{is_asset_slug, is_evm_address};

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

pub(super) fn validate_address(value: Option<&Value>) -> Result<String, ApiError> {
    let Some(Value::String(address)) = value else {
        return Err(ApiError::invalid_address());
    };

    if address.trim().is_empty() || !is_evm_address(address) {
        return Err(ApiError::invalid_address());
    }

    Ok(address.to_ascii_lowercase())
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

pub(super) fn validate_required_string(value: Option<&Value>) -> Result<String, ApiError> {
    match value {
        Some(Value::String(s)) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                Err(ApiError::invalid_request())
            } else {
                Ok(trimmed.to_string())
            }
        }
        _ => Err(ApiError::invalid_request()),
    }
}

pub(super) fn validate_optional_string(value: Option<&Value>) -> Result<Option<String>, ApiError> {
    match value {
        None => Ok(None),
        Some(Value::String(s)) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                Err(ApiError::invalid_request())
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        Some(_) => Err(ApiError::invalid_request()),
    }
}
