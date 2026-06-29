use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::adapters::http::error::ApiError;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TransferDirectionDTO {
    Any,
    From,
    To,
}

pub(crate) fn validate_direction(value: Option<&Value>) -> Result<TransferDirectionDTO, ApiError> {
    match value {
        Some(Value::String(direction)) => match direction.as_str() {
            "any" => Ok(TransferDirectionDTO::Any),
            "from" => Ok(TransferDirectionDTO::From),
            "to" => Ok(TransferDirectionDTO::To),
            _ => Err(ApiError::invalid_direction()),
        },
        _ => Err(ApiError::invalid_direction()),
    }
}
