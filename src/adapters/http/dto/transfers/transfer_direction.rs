use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::{
    adapters::http::error::ApiError, domain::transfers::transfer_direction::TransferDirection,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TransferDirectionDTO {
    Any,
    From,
    To,
}

impl From<TransferDirection> for TransferDirectionDTO {
    fn from(direction: TransferDirection) -> Self {
        match direction {
            TransferDirection::Any => Self::Any,
            TransferDirection::From => Self::From,
            TransferDirection::To => Self::To,
        }
    }
}

impl From<TransferDirectionDTO> for TransferDirection {
    fn from(direction: TransferDirectionDTO) -> Self {
        match direction {
            TransferDirectionDTO::Any => Self::Any,
            TransferDirectionDTO::From => Self::From,
            TransferDirectionDTO::To => Self::To,
        }
    }
}

impl TryFrom<Option<&Value>> for TransferDirectionDTO {
    type Error = ApiError;

    fn try_from(value: Option<&Value>) -> Result<Self, Self::Error> {
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
}
