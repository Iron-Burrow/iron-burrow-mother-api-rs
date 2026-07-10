use serde::{Deserialize, Serialize};

use crate::{
    adapters::http::{dto::onchain_time::as_of::AsOfRequest, error::ApiError},
    common::rfc3339::parse_rfc3339,
    domain::onchain_time::as_of::AsOf,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum BigwigAsOfDTO {
    Latest,
    Timestamp { timestamp: String },
    BlockNumber { block_number: String },
}

impl From<&AsOf> for BigwigAsOfDTO {
    fn from(as_of: &AsOf) -> Self {
        match as_of {
            AsOf::Latest => Self::Latest,
            AsOf::Timestamp { timestamp } => Self::Timestamp {
                timestamp: timestamp.clone(),
            },
            AsOf::BlockNumber { block_number } => Self::BlockNumber {
                block_number: block_number.clone(),
            },
        }
    }
}

impl TryFrom<AsOfRequest> for AsOf {
    type Error = ApiError;

    fn try_from(as_of: AsOfRequest) -> Result<Self, Self::Error> {
        match as_of.kind.as_str() {
            "latest" if as_of.timestamp.is_none() && as_of.block_number.is_none() => {
                Ok(AsOf::Latest)
            }
            "timestamp" if as_of.block_number.is_none() => {
                let Some(timestamp) = as_of.timestamp else {
                    return Err(ApiError::invalid_request());
                };
                if parse_rfc3339(&timestamp).is_none() {
                    return Err(ApiError::invalid_request());
                }
                Ok(AsOf::Timestamp { timestamp })
            }
            "block_number" if as_of.timestamp.is_none() => {
                let Some(block_number) = as_of.block_number else {
                    return Err(ApiError::invalid_request());
                };
                if !block_number
                    .as_bytes()
                    .iter()
                    .all(|character| character.is_ascii_digit())
                {
                    return Err(ApiError::invalid_request());
                }
                Ok(AsOf::BlockNumber { block_number })
            }
            "latest" | "timestamp" | "block_number" => Err(ApiError::invalid_request()),
            _ => Err(ApiError::unsupported_as_of()),
        }
    }
}
