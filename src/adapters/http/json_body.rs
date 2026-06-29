use serde_json::Value;

use super::types::JsonObject;
use crate::adapters::http::error::ApiError;

pub(crate) fn parse_json_object_body(body: &[u8]) -> Result<JsonObject, ApiError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| ApiError::invalid_json())?;

    let Value::Object(object) = value else {
        return Err(ApiError::invalid_json());
    };

    Ok(object)
}
