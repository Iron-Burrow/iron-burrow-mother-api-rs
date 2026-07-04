use crate::adapters::http::error::ApiError;
use crate::adapters::http::validation::{
    reject_unknown_fields, validate_optional_string, validate_required_string,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

const AS_OF_FIELDS: [&str; 3] = ["kind", "timestamp", "block_number"];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AsOfRequest {
    pub(crate) kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) block_number: Option<String>,
}

pub(crate) fn validate_as_of_object(value: Option<&Value>) -> Result<AsOfRequest, ApiError> {
    let Some(Value::Object(as_of)) = value else {
        return Err(ApiError::invalid_request());
    };

    reject_unknown_fields(as_of, &AS_OF_FIELDS)?;

    Ok(AsOfRequest {
        kind: validate_required_string(as_of.get("kind"))?,
        timestamp: validate_optional_string(as_of.get("timestamp"))?,
        block_number: validate_optional_string(as_of.get("block_number"))?,
    })
}
